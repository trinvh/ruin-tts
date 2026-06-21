import { createElement, useState } from "react";

type Tag = "div" | "button" | "span";

interface HoverBoxProps extends Omit<React.HTMLAttributes<HTMLElement>, "style"> {
  as?: Tag;
  style?: React.CSSProperties;
  hoverStyle?: React.CSSProperties;
  activeStyle?: React.CSSProperties;
  title?: string;
  disabled?: boolean;
  children?: React.ReactNode;
}

/**
 * Inline-styled box that merges `hoverStyle`/`activeStyle` on the matching
 * pointer state — the React equivalent of the design's `style-hover` /
 * `style-active` attributes.
 */
export function HoverBox({ as = "div", style, hoverStyle, activeStyle, children, disabled, onPointerDown, ...rest }: HoverBoxProps) {
  const [hover, setHover] = useState(false);
  const [active, setActive] = useState(false);
  const merged: React.CSSProperties = {
    ...style,
    ...(hover && !disabled ? hoverStyle : null),
    ...(active && !disabled ? activeStyle : null),
  };
  return createElement(
    as,
    {
      ...rest,
      disabled: as === "button" ? disabled : undefined,
      style: merged,
      onMouseEnter: () => setHover(true),
      onMouseLeave: () => {
        setHover(false);
        setActive(false);
      },
      onPointerDown: (e: React.PointerEvent<HTMLElement>) => {
        setActive(true);
        onPointerDown?.(e);
      },
      onPointerUp: () => setActive(false),
    },
    children,
  );
}
