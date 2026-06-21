//! JSON-serializable workflow graph + a pluggable async handler registry.
//! Adding a node type is just registering a new handler.

use std::collections::HashMap;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::future::{BoxFuture, FutureExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeDef {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: String,
    #[serde(default)]
    pub config: Value,
    #[serde(default)]
    pub position: Option<Position>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeDef {
    pub from: String,
    pub to: String,
    /// Source port the edge leaves from. Empty/None = the node's default output.
    /// `If` uses "then"/"else"; `Loop` uses "body"/"done".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handle: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDef {
    pub id: String,
    pub name: String,
    pub version: u32,
    pub nodes: Vec<NodeDef>,
    pub edges: Vec<EdgeDef>,
}

/// Mutable state threaded through a run; values are JSON for serializability.
#[derive(Debug, Default)]
pub struct RunContext {
    data: HashMap<String, Value>,
    pub logs: Vec<String>,
}

impl RunContext {
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.data.get(key)
    }
    pub fn set(&mut self, key: impl Into<String>, value: Value) {
        self.data.insert(key.into(), value);
    }
    pub fn log(&mut self, msg: impl Into<String>) {
        self.logs.push(msg.into());
    }

    /// A compact, viewable snapshot of the context (long text/arrays truncated;
    /// media paths preserved so the UI can show/play outputs).
    pub fn data_summary(&self) -> Value {
        Value::Object(
            self.data
                .iter()
                .map(|(k, v)| (k.clone(), summarize_value(v)))
                .collect(),
        )
    }

    /// Full, untruncated context (used to checkpoint for node retry/resume).
    pub fn data_full(&self) -> Value {
        Value::Object(
            self.data
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        )
    }

    /// Restore the context from a previously captured `data_full` snapshot.
    pub fn from_json(value: &Value) -> Self {
        let mut ctx = Self::default();
        if let Value::Object(map) = value {
            for (k, v) in map {
                ctx.data.insert(k.clone(), v.clone());
            }
        }
        ctx
    }
}

fn summarize_value(v: &Value) -> Value {
    match v {
        Value::String(s) if s.chars().count() > 300 => {
            let head: String = s.chars().take(300).collect();
            Value::String(format!("{head}… ({} ký tự)", s.chars().count()))
        }
        Value::Array(a) if a.len() > 6 => {
            let mut out: Vec<Value> = a.iter().take(3).map(summarize_value).collect();
            out.push(Value::String(format!("… +{} mục", a.len() - 3)));
            Value::Array(out)
        }
        Value::Array(a) => Value::Array(a.iter().map(summarize_value).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, v)| (k.clone(), summarize_value(v)))
                .collect(),
        ),
        _ => v.clone(),
    }
}

/// Observes a run step-by-step (used to persist live progress + I/O). `step_id`
/// is the node id for normal steps, or `node_id#<iter>` for loop-body steps, so
/// each loop iteration is its own retriable/visible step.
#[async_trait]
pub trait RunObserver: Send + Sync {
    async fn on_start(&self, step_id: &str, node: &NodeDef, seq: usize, input: &Value);
    async fn on_finish(
        &self,
        step_id: &str,
        node: &NodeDef,
        output: &Value,
        ctx_state: &Value,
        error: Option<&str>,
    );
}

/// Branching executor: walks the graph following edge handles, evaluating `If`
/// (then/else) and expanding `Loop` (one body pass per array item). Normal nodes
/// run via the registry. Supports the structured shapes the editor produces
/// (linear backbone + If diamonds + Loop blocks).
pub async fn execute_workflow_observed(
    wf: &WorkflowDef,
    registry: &Registry,
    ctx: &mut RunContext,
    observer: &dyn RunObserver,
) -> Result<()> {
    // Owned adjacency keyed by (from, handle) so recursion is lifetime-simple.
    let nodes: HashMap<String, NodeDef> =
        wf.nodes.iter().map(|n| (n.id.clone(), n.clone())).collect();
    let mut out: HashMap<(String, String), Vec<String>> = HashMap::new();
    let mut indeg: HashMap<&str, usize> = nodes.keys().map(|k| (k.as_str(), 0)).collect();
    for e in &wf.edges {
        let h = e.handle.clone().unwrap_or_default();
        out.entry((e.from.clone(), h))
            .or_default()
            .push(e.to.clone());
        *indeg.entry(e.to.as_str()).or_default() += 1;
    }
    let starts: Vec<String> = wf
        .nodes
        .iter()
        .filter(|n| indeg.get(n.id.as_str()).copied().unwrap_or(0) == 0)
        .map(|n| n.id.clone())
        .collect();

    let mut executed: std::collections::HashSet<String> = Default::default();
    let seq = std::sync::atomic::AtomicUsize::new(0);
    for s in starts {
        walk(
            s,
            String::new(),
            None,
            ctx,
            &nodes,
            &out,
            registry,
            observer,
            &mut executed,
            &seq,
        )
        .await?;
    }
    Ok(())
}

/// Build node + (from, handle)→targets adjacency maps from a graph.
fn build_maps(
    wf: &WorkflowDef,
) -> (
    HashMap<String, NodeDef>,
    HashMap<(String, String), Vec<String>>,
) {
    let nodes = wf.nodes.iter().map(|n| (n.id.clone(), n.clone())).collect();
    let mut out: HashMap<(String, String), Vec<String>> = HashMap::new();
    for e in &wf.edges {
        let h = e.handle.clone().unwrap_or_default();
        out.entry((e.from.clone(), h))
            .or_default()
            .push(e.to.clone());
    }
    (nodes, out)
}

/// Resume execution starting at a specific node, with a step-id suffix and an
/// optional enclosing-loop id (used to retry a single loop iteration's body).
pub async fn execute_from(
    wf: &WorkflowDef,
    registry: &Registry,
    ctx: &mut RunContext,
    observer: &dyn RunObserver,
    start: &str,
    suffix: &str,
    loop_stop: Option<String>,
) -> Result<()> {
    let (nodes, out) = build_maps(wf);
    let mut executed: std::collections::HashSet<String> = Default::default();
    let seq = std::sync::atomic::AtomicUsize::new(0);
    walk(
        start.to_string(),
        suffix.to_string(),
        loop_stop,
        ctx,
        &nodes,
        &out,
        registry,
        observer,
        &mut executed,
        &seq,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
fn walk<'a>(
    node_id: String,
    suffix: String,
    loop_stop: Option<String>,
    ctx: &'a mut RunContext,
    nodes: &'a HashMap<String, NodeDef>,
    out: &'a HashMap<(String, String), Vec<String>>,
    registry: &'a Registry,
    observer: &'a dyn RunObserver,
    executed: &'a mut std::collections::HashSet<String>,
    seq: &'a std::sync::atomic::AtomicUsize,
) -> BoxFuture<'a, Result<()>> {
    use std::sync::atomic::Ordering::Relaxed;
    async move {
        let step_id = format!("{node_id}{suffix}");
        if executed.contains(&step_id) {
            return Ok(());
        }
        let node = nodes
            .get(&node_id)
            .ok_or_else(|| anyhow!("edge points to unknown node \"{node_id}\""))?;

        match node.node_type.as_str() {
            "Loop" => {
                observer
                    .on_start(&step_id, node, seq.fetch_add(1, Relaxed), &node.config)
                    .await;
                let over = node
                    .config
                    .get("over")
                    .and_then(|v| v.as_str())
                    .unwrap_or("videos")
                    .to_string();
                let items: Vec<Value> = ctx
                    .get(&over)
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                ctx.log(format!("Lặp {} mục từ '{}'", items.len(), over));
                // Preserve any enclosing loop's index so nested loops restore it.
                let prior_index = ctx.get("__loop_index").cloned();
                let body = out
                    .get(&(node_id.clone(), "body".to_string()))
                    .cloned()
                    .unwrap_or_default();
                let mut results: Vec<Value> = Vec::with_capacity(items.len());
                let mut loop_err: Option<anyhow::Error> = None;
                for (i, item) in items.into_iter().enumerate() {
                    ctx.set(&over, Value::Array(vec![item.clone()]));
                    ctx.set("__loop_index", serde_json::json!(i));
                    let isuf = format!("{suffix}#{}", i + 1);
                    for b in &body {
                        if let Err(e) = walk(
                            b.clone(),
                            isuf.clone(),
                            Some(node_id.clone()),
                            ctx,
                            nodes,
                            out,
                            registry,
                            observer,
                            executed,
                            seq,
                        )
                        .await
                        {
                            loop_err = Some(e);
                            break;
                        }
                    }
                    let collected = ctx
                        .get(&over)
                        .and_then(|v| v.as_array())
                        .and_then(|a| a.first())
                        .cloned()
                        .unwrap_or(item);
                    results.push(collected);
                    if loop_err.is_some() {
                        break;
                    }
                }
                ctx.set(&over, Value::Array(results));
                // Restore the enclosing index so post-loop nodes don't see a stale one.
                ctx.set("__loop_index", prior_index.unwrap_or(Value::Null));
                let ctx_state = ctx.data_full();
                let output = serde_json::json!({ "logs": [], "state": ctx.data_summary() });
                executed.insert(step_id.clone());
                observer
                    .on_finish(
                        &step_id,
                        node,
                        &output,
                        &ctx_state,
                        loop_err.as_ref().map(|e| format!("{e:#}")).as_deref(),
                    )
                    .await;
                if let Some(e) = loop_err {
                    return Err(e);
                }
                follow(
                    out, &node_id, "done", &suffix, &loop_stop, ctx, nodes, registry, observer,
                    executed, seq,
                )
                .await
            }
            "If" => {
                observer
                    .on_start(&step_id, node, seq.fetch_add(1, Relaxed), &node.config)
                    .await;
                let taken = eval_condition(&node.config, ctx);
                ctx.log(format!("Điều kiện → nhánh '{taken}'"));
                let ctx_state = ctx.data_full();
                let output =
                    serde_json::json!({ "logs": [], "state": ctx.data_summary(), "branch": taken });
                executed.insert(step_id.clone());
                observer
                    .on_finish(&step_id, node, &output, &ctx_state, None)
                    .await;
                follow(
                    out, &node_id, &taken, &suffix, &loop_stop, ctx, nodes, registry, observer,
                    executed, seq,
                )
                .await
            }
            _ => {
                let handler = registry.get(&node.node_type).ok_or_else(|| {
                    anyhow!("no handler registered for node type \"{}\"", node.node_type)
                })?;
                observer
                    .on_start(&step_id, node, seq.fetch_add(1, Relaxed), &node.config)
                    .await;
                let log_start = ctx.logs.len();
                let res = handler.run(node, ctx).await;
                let new_logs: Vec<Value> = ctx.logs[log_start..]
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect();
                let output = serde_json::json!({ "logs": new_logs, "state": ctx.data_summary() });
                let ctx_state = ctx.data_full();
                executed.insert(step_id.clone());
                observer
                    .on_finish(
                        &step_id,
                        node,
                        &output,
                        &ctx_state,
                        res.as_ref().err().map(|e| format!("{e:#}")).as_deref(),
                    )
                    .await;
                res?;
                follow(
                    out, &node_id, "", &suffix, &loop_stop, ctx, nodes, registry, observer,
                    executed, seq,
                )
                .await
            }
        }
    }
    .boxed()
}

/// Follow the successors of `node_id` on `handle`, skipping a back-edge to the
/// enclosing loop (which signals end-of-body, not a node to run).
#[allow(clippy::too_many_arguments)]
fn follow<'a>(
    out: &'a HashMap<(String, String), Vec<String>>,
    node_id: &'a str,
    handle: &'a str,
    suffix: &'a str,
    loop_stop: &'a Option<String>,
    ctx: &'a mut RunContext,
    nodes: &'a HashMap<String, NodeDef>,
    registry: &'a Registry,
    observer: &'a dyn RunObserver,
    executed: &'a mut std::collections::HashSet<String>,
    seq: &'a std::sync::atomic::AtomicUsize,
) -> BoxFuture<'a, Result<()>> {
    async move {
        let targets = out
            .get(&(node_id.to_string(), handle.to_string()))
            .cloned()
            .unwrap_or_default();
        for t in targets {
            if loop_stop.as_deref() == Some(t.as_str()) {
                continue; // back-edge to the loop header → end of this body pass
            }
            walk(
                t,
                suffix.to_string(),
                loop_stop.clone(),
                ctx,
                nodes,
                out,
                registry,
                observer,
                executed,
                seq,
            )
            .await?;
        }
        Ok(())
    }
    .boxed()
}

/// Evaluate an `If` node's condition against the context, returning "then"/"else".
fn eval_condition(cfg: &Value, ctx: &RunContext) -> String {
    let key = cfg.get("key").and_then(|v| v.as_str()).unwrap_or("");
    let op = cfg.get("op").and_then(|v| v.as_str()).unwrap_or("nonempty");
    let val = ctx.get(key);
    let truthy = match op {
        "nonempty" => match val {
            Some(Value::Array(a)) => !a.is_empty(),
            Some(Value::String(s)) => !s.is_empty(),
            Some(Value::Null) | None => false,
            Some(_) => true,
        },
        "empty" => match val {
            Some(Value::Array(a)) => a.is_empty(),
            Some(Value::String(s)) => s.is_empty(),
            Some(Value::Null) | None => true,
            Some(_) => false,
        },
        "truthy" => match val {
            Some(Value::Bool(b)) => *b,
            Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0) != 0.0,
            Some(Value::String(s)) => !s.is_empty(),
            _ => false,
        },
        "eq" | "ne" | "gt" | "lt" => {
            let rhs = cfg.get("value");
            let ord = compare_json(val, rhs);
            match (op, ord) {
                ("eq", Some(o)) => o == std::cmp::Ordering::Equal,
                ("ne", Some(o)) => o != std::cmp::Ordering::Equal,
                ("ne", None) => true,
                ("gt", Some(o)) => o == std::cmp::Ordering::Greater,
                ("lt", Some(o)) => o == std::cmp::Ordering::Less,
                _ => false,
            }
        }
        _ => true,
    };
    if truthy {
        "then".into()
    } else {
        "else".into()
    }
}

fn compare_json(a: Option<&Value>, b: Option<&Value>) -> Option<std::cmp::Ordering> {
    let (a, b) = (a?, b?);
    if let (Some(x), Some(y)) = (json_num(a), json_num(b)) {
        return x.partial_cmp(&y);
    }
    Some(a.to_string().cmp(&b.to_string()))
}
fn json_num(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

#[async_trait]
pub trait NodeHandler: Send + Sync {
    fn node_type(&self) -> &str;
    async fn run(&self, node: &NodeDef, ctx: &mut RunContext) -> Result<()>;
}

#[derive(Default)]
pub struct Registry {
    handlers: HashMap<String, Box<dyn NodeHandler>>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn register(&mut self, handler: Box<dyn NodeHandler>) -> &mut Self {
        self.handlers
            .insert(handler.node_type().to_string(), handler);
        self
    }
    pub fn get(&self, node_type: &str) -> Option<&dyn NodeHandler> {
        self.handlers.get(node_type).map(|b| b.as_ref())
    }
    pub fn has(&self, node_type: &str) -> bool {
        self.handlers.contains_key(node_type)
    }
}

/// Kahn's algorithm; preserves declaration order among independent nodes.
pub fn topo_sort(wf: &WorkflowDef) -> Result<Vec<NodeDef>> {
    let ids: std::collections::HashSet<&str> = wf.nodes.iter().map(|n| n.id.as_str()).collect();
    let mut indeg: HashMap<&str, usize> = wf.nodes.iter().map(|n| (n.id.as_str(), 0)).collect();
    let mut adj: HashMap<&str, Vec<&str>> = wf
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), Vec::new()))
        .collect();

    for e in &wf.edges {
        if !ids.contains(e.from.as_str()) || !ids.contains(e.to.as_str()) {
            return Err(anyhow!(
                "edge references unknown node: {} → {}",
                e.from,
                e.to
            ));
        }
        adj.get_mut(e.from.as_str()).unwrap().push(e.to.as_str());
        *indeg.get_mut(e.to.as_str()).unwrap() += 1;
    }

    let mut queue: std::collections::VecDeque<&str> = wf
        .nodes
        .iter()
        .filter(|n| indeg[n.id.as_str()] == 0)
        .map(|n| n.id.as_str())
        .collect();
    let mut order: Vec<&str> = Vec::new();
    while let Some(id) = queue.pop_front() {
        order.push(id);
        for &next in &adj[id] {
            let d = indeg.get_mut(next).unwrap();
            *d -= 1;
            if *d == 0 {
                queue.push_back(next);
            }
        }
    }

    if order.len() != wf.nodes.len() {
        return Err(anyhow!("workflow graph has a cycle"));
    }
    let by_id: HashMap<&str, &NodeDef> = wf.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    Ok(order.into_iter().map(|id| by_id[id].clone()).collect())
}

pub async fn execute_workflow(
    wf: &WorkflowDef,
    registry: &Registry,
    ctx: &mut RunContext,
) -> Result<()> {
    for node in topo_sort(wf)? {
        let handler = registry
            .get(&node.node_type)
            .ok_or_else(|| anyhow!("no handler registered for node type \"{}\"", node.node_type))?;
        ctx.log(format!("▶ {} ({})", node.node_type, node.id));
        handler.run(&node, ctx).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn wf(nodes: &[(&str, &str)], edges: &[(&str, &str)]) -> WorkflowDef {
        WorkflowDef {
            id: "w".into(),
            name: "t".into(),
            version: 1,
            nodes: nodes
                .iter()
                .map(|(id, t)| NodeDef {
                    id: id.to_string(),
                    node_type: t.to_string(),
                    config: Value::Null,
                    position: None,
                })
                .collect(),
            edges: edges
                .iter()
                .map(|(f, t)| EdgeDef {
                    from: f.to_string(),
                    to: t.to_string(),
                    handle: None,
                })
                .collect(),
        }
    }

    #[test]
    fn linear_chain_order() {
        let o = topo_sort(&wf(
            &[("a", "A"), ("b", "B"), ("c", "C")],
            &[("a", "b"), ("b", "c")],
        ))
        .unwrap();
        assert_eq!(
            o.iter().map(|n| n.id.clone()).collect::<Vec<_>>(),
            vec!["a", "b", "c"]
        );
    }

    #[test]
    fn detects_cycle() {
        let e = topo_sort(&wf(&[("a", "A"), ("b", "B")], &[("a", "b"), ("b", "a")])).unwrap_err();
        assert!(e.to_string().contains("cycle"));
    }

    #[test]
    fn unknown_edge() {
        let e = topo_sort(&wf(&[("a", "A")], &[("a", "ghost")])).unwrap_err();
        assert!(e.to_string().contains("unknown"));
    }

    struct TraceHandler {
        t: String,
        trace: Arc<Mutex<Vec<String>>>,
    }
    #[async_trait]
    impl NodeHandler for TraceHandler {
        fn node_type(&self) -> &str {
            &self.t
        }
        async fn run(&self, _node: &NodeDef, ctx: &mut RunContext) -> Result<()> {
            self.trace.lock().unwrap().push(self.t.clone());
            ctx.set("last", Value::String(self.t.clone()));
            Ok(())
        }
    }

    #[tokio::test]
    async fn executes_in_topo_order() {
        let trace = Arc::new(Mutex::new(Vec::new()));
        let mut reg = Registry::new();
        for t in ["A", "B", "C"] {
            reg.register(Box::new(TraceHandler {
                t: t.into(),
                trace: trace.clone(),
            }));
        }
        let mut ctx = RunContext::default();
        execute_workflow(
            &wf(
                &[("a", "A"), ("b", "B"), ("c", "C")],
                &[("a", "b"), ("b", "c")],
            ),
            &reg,
            &mut ctx,
        )
        .await
        .unwrap();
        assert_eq!(*trace.lock().unwrap(), vec!["A", "B", "C"]);
        assert_eq!(ctx.get("last"), Some(&Value::String("C".into())));
    }

    #[tokio::test]
    async fn missing_handler_errors() {
        let reg = Registry::new();
        let mut ctx = RunContext::default();
        let e = execute_workflow(&wf(&[("a", "Missing")], &[]), &reg, &mut ctx)
            .await
            .unwrap_err();
        assert!(e.to_string().contains("no handler"));
    }

    // ── Branching executor (If / Loop) ────────────────────────────────────────
    struct RecObserver {
        steps: Arc<Mutex<Vec<String>>>,
    }
    #[async_trait]
    impl RunObserver for RecObserver {
        async fn on_start(&self, step_id: &str, _n: &NodeDef, _seq: usize, _i: &Value) {
            self.steps.lock().unwrap().push(step_id.to_string());
        }
        async fn on_finish(
            &self,
            _s: &str,
            _n: &NodeDef,
            _o: &Value,
            _c: &Value,
            _e: Option<&str>,
        ) {
        }
    }
    /// Records its run (with the loop index, if any) into a shared vec.
    struct RecHandler {
        t: String,
        rec: Arc<Mutex<Vec<String>>>,
    }
    #[async_trait]
    impl NodeHandler for RecHandler {
        fn node_type(&self) -> &str {
            &self.t
        }
        async fn run(&self, _node: &NodeDef, ctx: &mut RunContext) -> Result<()> {
            let tag = match ctx.get("__loop_index").and_then(|v| v.as_u64()) {
                Some(i) => format!("{}:{i}", self.t),
                None => self.t.clone(),
            };
            self.rec.lock().unwrap().push(tag);
            Ok(())
        }
    }

    fn node(id: &str, t: &str, cfg: Value) -> NodeDef {
        NodeDef {
            id: id.into(),
            node_type: t.into(),
            config: cfg,
            position: None,
        }
    }
    fn edge(f: &str, t: &str, h: Option<&str>) -> EdgeDef {
        EdgeDef {
            from: f.into(),
            to: t.into(),
            handle: h.map(|s| s.into()),
        }
    }

    #[tokio::test]
    async fn if_takes_then_branch_only() {
        let rec = Arc::new(Mutex::new(Vec::new()));
        let mut reg = Registry::new();
        for t in ["T", "E"] {
            reg.register(Box::new(RecHandler {
                t: t.into(),
                rec: rec.clone(),
            }));
        }
        let g = WorkflowDef {
            id: "w".into(),
            name: "t".into(),
            version: 1,
            nodes: vec![
                node(
                    "a",
                    "If",
                    serde_json::json!({ "key": "flag", "op": "truthy" }),
                ),
                node("t", "T", Value::Null),
                node("e", "E", Value::Null),
            ],
            edges: vec![edge("a", "t", Some("then")), edge("a", "e", Some("else"))],
        };
        let mut ctx = RunContext::default();
        ctx.set("flag", Value::Bool(true));
        let obs = RecObserver {
            steps: Arc::new(Mutex::new(Vec::new())),
        };
        execute_workflow_observed(&g, &reg, &mut ctx, &obs)
            .await
            .unwrap();
        assert_eq!(*rec.lock().unwrap(), vec!["T"]); // else branch skipped
    }

    #[tokio::test]
    async fn loop_runs_body_per_item_then_done() {
        let rec = Arc::new(Mutex::new(Vec::new()));
        let steps = Arc::new(Mutex::new(Vec::new()));
        let mut reg = Registry::new();
        for t in ["START", "BODY", "DONE"] {
            reg.register(Box::new(RecHandler {
                t: t.into(),
                rec: rec.clone(),
            }));
        }
        let g = WorkflowDef {
            id: "w".into(),
            name: "t".into(),
            version: 1,
            nodes: vec![
                node("s", "START", Value::Null),
                node("l", "Loop", serde_json::json!({ "over": "videos" })),
                node("b", "BODY", Value::Null),
                node("d", "DONE", Value::Null),
            ],
            edges: vec![
                edge("s", "l", None),
                edge("l", "b", Some("body")),
                edge("l", "d", Some("done")),
            ],
        };
        let mut ctx = RunContext::default();
        ctx.set("videos", serde_json::json!([10, 20]));
        let obs = RecObserver {
            steps: steps.clone(),
        };
        execute_workflow_observed(&g, &reg, &mut ctx, &obs)
            .await
            .unwrap();
        // body runs once per item (with index), then done
        assert_eq!(
            *rec.lock().unwrap(),
            vec!["START", "BODY:0", "BODY:1", "DONE"]
        );
        // each iteration is its own step id
        let s = steps.lock().unwrap();
        assert!(s.contains(&"b#1".to_string()));
        assert!(s.contains(&"b#2".to_string()));
        // the array is reassembled after the loop
        assert_eq!(ctx.get("videos"), Some(&serde_json::json!([10, 20])));
    }
}
