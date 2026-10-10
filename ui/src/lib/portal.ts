// Moves an element to the end of <body>. A tile clips what overflows it and sits inside a
// transform, so its menus and pop-ups are drawn outside it, at a fixed place on the screen.

export function portal(node: HTMLElement) {
  document.body.appendChild(node);
  return {
    destroy() {
      node.remove();
    },
  };
}
