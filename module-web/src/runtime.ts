import type * as ReactRuntime from "react";

let injected: typeof ReactRuntime | null = null;

export function bindReact(runtime: typeof ReactRuntime): void {
  if (injected && injected !== runtime) {
    throw new Error("模块 React Runtime 不能在激活后替换");
  }
  injected = runtime;
}

function react(): typeof ReactRuntime {
  if (!injected) throw new Error("模块尚未由 Union Web Shell 激活");
  return injected;
}

export const Fragment = Symbol.for("react.fragment") as unknown as typeof ReactRuntime.Fragment;
export const createElement = ((...args: unknown[]) =>
  (react().createElement as (...values: unknown[]) => unknown)(...args)) as typeof ReactRuntime.createElement;
export const cloneElement = ((...args: unknown[]) =>
  (react().cloneElement as (...values: unknown[]) => unknown)(...args)) as typeof ReactRuntime.cloneElement;
export const createContext = ((...args: unknown[]) =>
  (react().createContext as (...values: unknown[]) => unknown)(...args)) as typeof ReactRuntime.createContext;
export const forwardRef = ((...args: unknown[]) =>
  (react().forwardRef as (...values: unknown[]) => unknown)(...args)) as typeof ReactRuntime.forwardRef;
export const memo = ((...args: unknown[]) =>
  (react().memo as (...values: unknown[]) => unknown)(...args)) as typeof ReactRuntime.memo;
export const useCallback = ((...args: unknown[]) =>
  (react().useCallback as (...values: unknown[]) => unknown)(...args)) as typeof ReactRuntime.useCallback;
export const useContext = ((...args: unknown[]) =>
  (react().useContext as (...values: unknown[]) => unknown)(...args)) as typeof ReactRuntime.useContext;
export const useEffect = ((...args: unknown[]) =>
  (react().useEffect as (...values: unknown[]) => unknown)(...args)) as typeof ReactRuntime.useEffect;
export const useId = ((...args: unknown[]) =>
  (react().useId as (...values: unknown[]) => unknown)(...args)) as typeof ReactRuntime.useId;
export const useLayoutEffect = ((...args: unknown[]) =>
  (react().useLayoutEffect as (...values: unknown[]) => unknown)(...args)) as typeof ReactRuntime.useLayoutEffect;
export const useMemo = ((...args: unknown[]) =>
  (react().useMemo as (...values: unknown[]) => unknown)(...args)) as typeof ReactRuntime.useMemo;
export const useReducer = ((...args: unknown[]) =>
  (react().useReducer as (...values: unknown[]) => unknown)(...args)) as typeof ReactRuntime.useReducer;
export const useRef = ((...args: unknown[]) =>
  (react().useRef as (...values: unknown[]) => unknown)(...args)) as typeof ReactRuntime.useRef;
export const useState = ((...args: unknown[]) =>
  (react().useState as (...values: unknown[]) => unknown)(...args)) as typeof ReactRuntime.useState;
export const useSyncExternalStore = ((...args: unknown[]) =>
  (react().useSyncExternalStore as (...values: unknown[]) => unknown)(...args)) as typeof ReactRuntime.useSyncExternalStore;

const defaultRuntime = {
  Fragment,
  createElement,
  cloneElement,
  createContext,
  forwardRef,
  memo,
  useCallback,
  useContext,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
  useSyncExternalStore,
};

export default defaultRuntime;
