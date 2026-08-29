import { Fragment, createElement } from "./runtime";

export { Fragment };

export function jsx(
  type: Parameters<typeof createElement>[0],
  props: Record<string, unknown> | null,
  key?: string,
) {
  return createElement(type, key === undefined ? props : { ...props, key });
}

export const jsxs = jsx;
export const jsxDEV = jsx;
