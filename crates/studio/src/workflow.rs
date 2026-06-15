//! JSON-serializable workflow graph + a pluggable async handler registry.
//! Adding a node type is just registering a new handler.

use std::collections::HashMap;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
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

/// Observes a run node-by-node (used to persist live progress + I/O).
#[async_trait]
pub trait RunObserver: Send + Sync {
    async fn on_start(&self, node: &NodeDef, input: &Value);
    async fn on_finish(
        &self,
        node: &NodeDef,
        output: &Value,
        ctx_state: &Value,
        error: Option<&str>,
    );
}

/// Like [`execute_workflow`] but reports per-node start/finish with the node's
/// config (input) and a snapshot of what it produced (output + logs).
pub async fn execute_workflow_observed(
    wf: &WorkflowDef,
    registry: &Registry,
    ctx: &mut RunContext,
    observer: &dyn RunObserver,
) -> Result<()> {
    for node in topo_sort(wf)? {
        let handler = registry
            .get(&node.node_type)
            .ok_or_else(|| anyhow!("no handler registered for node type \"{}\"", node.node_type))?;
        observer.on_start(&node, &node.config).await;
        let log_start = ctx.logs.len();
        let res = handler.run(&node, ctx).await;
        let new_logs: Vec<Value> = ctx.logs[log_start..]
            .iter()
            .cloned()
            .map(Value::String)
            .collect();
        let output = serde_json::json!({ "logs": new_logs, "state": ctx.data_summary() });
        let ctx_state = ctx.data_full();
        observer
            .on_finish(
                &node,
                &output,
                &ctx_state,
                res.as_ref().err().map(|e| format!("{e:#}")).as_deref(),
            )
            .await;
        res?;
    }
    Ok(())
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
}
