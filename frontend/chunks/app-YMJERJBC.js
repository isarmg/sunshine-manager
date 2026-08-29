import {
  Fragment,
  applySunshineHostPatch,
  createContext,
  createElement,
  forwardRef,
  isOptimisticSunshineHost,
  mergeSunshineHostSnapshot,
  optimisticSunshineHost,
  parseSunshineConfigDraft,
  persistedSunshineHosts,
  removeSunshineHost,
  replaceSunshineHost,
  restoreSunshineHost,
  sunshineApi,
  sunshineHostMutationKeys,
  sunshineHostsRefetchInterval,
  sunshineLogLines,
  useCallback,
  useContext,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore
} from "./chunk-DMRF6SCT.js";

// src/jsx-runtime.ts
function jsx(type, props, key) {
  return createElement(type, key === void 0 ? props : { ...props, key });
}
var jsxs = jsx;

// node_modules/@tanstack/react-query/build/modern/QueryClientProvider.js
var QueryClientContext = createContext(void 0);
var useQueryClient = (queryClient) => {
  const client = useContext(QueryClientContext);
  if (queryClient) return queryClient;
  if (!client) throw new Error("No QueryClient set, use QueryClientProvider to set one");
  return client;
};
var QueryClientProvider = ({ client, children }) => {
  useEffect(() => {
    client.mount();
    return () => {
      client.unmount();
    };
  }, [client]);
  return /* @__PURE__ */ jsx(QueryClientContext.Provider, {
    value: client,
    children
  });
};

// node_modules/@tanstack/query-core/build/modern/timeoutManager.js
var defaultTimeoutProvider = {
  setTimeout: (callback, delay) => setTimeout(callback, delay),
  clearTimeout: (timeoutId) => clearTimeout(timeoutId),
  setInterval: (callback, delay) => setInterval(callback, delay),
  clearInterval: (intervalId) => clearInterval(intervalId)
};
var TimeoutManager = class {
  #provider = defaultTimeoutProvider;
  #providerCalled = false;
  setTimeoutProvider(provider) {
    if (true) {
      if (this.#providerCalled && provider !== this.#provider) console.error(`[timeoutManager]: Switching provider after calls to previous provider might result in unexpected behavior.`, {
        previous: this.#provider,
        provider
      });
    }
    this.#provider = provider;
    if (true) this.#providerCalled = false;
  }
  setTimeout(callback, delay) {
    if (true) this.#providerCalled = true;
    return this.#provider.setTimeout(callback, delay);
  }
  clearTimeout(timeoutId) {
    this.#provider.clearTimeout(timeoutId);
  }
  setInterval(callback, delay) {
    if (true) this.#providerCalled = true;
    return this.#provider.setInterval(callback, delay);
  }
  clearInterval(intervalId) {
    this.#provider.clearInterval(intervalId);
  }
};
var timeoutManager = new TimeoutManager();
function systemSetTimeoutZero(callback) {
  setTimeout(callback, 0);
}

// node_modules/@tanstack/query-core/build/modern/utils.js
var isServer = typeof window === "undefined" || "Deno" in globalThis;
function noop() {
}
function functionalUpdate(updater, input) {
  return typeof updater === "function" ? updater(input) : updater;
}
function isValidTimeout(value) {
  return typeof value === "number" && value >= 0 && value !== Infinity;
}
function timeUntilStale(updatedAt, staleTime) {
  return Math.max(updatedAt + (staleTime || 0) - Date.now(), 0);
}
function resolveQueryValue(value, query) {
  return typeof value === "function" ? value(query) : value;
}
function matchQuery(filters, query) {
  const { type = "all", exact, fetchStatus, predicate, queryKey, stale } = filters;
  if (queryKey) {
    if (exact) {
      if (query.queryHash !== hashQueryKeyByOptions(queryKey, query.options)) return false;
    } else if (!partialMatchKey(query.queryKey, queryKey)) return false;
  }
  if (type !== "all") {
    const isActive = query.isActive();
    if (type === "active" && !isActive) return false;
    if (type === "inactive" && isActive) return false;
  }
  if (typeof stale === "boolean" && query.isStale() !== stale) return false;
  if (fetchStatus && fetchStatus !== query.state.fetchStatus) return false;
  if (predicate && !predicate(query)) return false;
  return true;
}
function matchMutation(filters, mutation) {
  const { exact, status, predicate, mutationKey } = filters;
  if (mutationKey) {
    if (!mutation.options.mutationKey) return false;
    if (exact) {
      if (hashKey(mutation.options.mutationKey) !== hashKey(mutationKey)) return false;
    } else if (!partialMatchKey(mutation.options.mutationKey, mutationKey)) return false;
  }
  if (status && mutation.state.status !== status) return false;
  if (predicate && !predicate(mutation)) return false;
  return true;
}
function hashQueryKeyByOptions(queryKey, options) {
  return (options?.queryKeyHashFn || hashKey)(queryKey);
}
function hashKey(queryKey) {
  return JSON.stringify(queryKey, (_, val) => isPlainObject(val) ? Object.keys(val).sort().reduce((result, key) => {
    result[key] = val[key];
    return result;
  }, {}) : val);
}
function partialMatchKey(a, b) {
  if (a === b) return true;
  if (typeof a !== typeof b) return false;
  if (a && b && typeof a === "object" && typeof b === "object") {
    if (Array.isArray(a) && Array.isArray(b)) {
      for (let i = 0; i < b.length; i++) if (!partialMatchKey(a[i], b[i])) return false;
      return true;
    }
    const bKeys = Object.keys(b);
    for (const key of bKeys) if (!partialMatchKey(a[key], b[key])) return false;
    return true;
  }
  return false;
}
var hasOwn = Object.prototype.hasOwnProperty;
function replaceEqualDeep(a, b, depth = 0) {
  if (a === b) return a;
  if (depth > 500) return b;
  const array = isPlainArray(a) && isPlainArray(b);
  if (!array && !(isPlainObject(a) && isPlainObject(b))) return b;
  const aSize = (array ? a : Object.keys(a)).length;
  const bItems = array ? b : Object.keys(b);
  const bSize = bItems.length;
  const copy = array ? new Array(bSize) : {};
  let equalItems = 0;
  for (let i = 0; i < bSize; i++) {
    const key = array ? i : bItems[i];
    const aItem = a[key];
    const bItem = b[key];
    if (aItem === bItem) {
      copy[key] = aItem;
      if (array ? i < aSize : hasOwn.call(a, key)) equalItems++;
      continue;
    }
    if (aItem === null || bItem === null || typeof aItem !== "object" || typeof bItem !== "object") {
      copy[key] = bItem;
      continue;
    }
    const v = replaceEqualDeep(aItem, bItem, depth + 1);
    copy[key] = v;
    if (v === aItem) equalItems++;
  }
  return aSize === bSize && equalItems === aSize ? a : copy;
}
function shallowEqualObjects(a, b) {
  if (!b || Object.keys(a).length !== Object.keys(b).length) return false;
  for (const key in a) if (a[key] !== b[key]) return false;
  return true;
}
function isPlainArray(value) {
  return Array.isArray(value) && value.length === Object.keys(value).length;
}
function isPlainObject(o) {
  if (!hasObjectPrototype(o)) return false;
  const ctor = o.constructor;
  if (ctor === void 0) return true;
  const prot = ctor.prototype;
  if (!hasObjectPrototype(prot)) return false;
  if (!prot.hasOwnProperty("isPrototypeOf")) return false;
  if (Object.getPrototypeOf(o) !== Object.prototype) return false;
  return true;
}
function hasObjectPrototype(o) {
  return Object.prototype.toString.call(o) === "[object Object]";
}
function sleep(timeout) {
  return new Promise((resolve) => {
    timeoutManager.setTimeout(resolve, timeout);
  });
}
function replaceData(prevData, data, options) {
  if (typeof options.structuralSharing === "function") return options.structuralSharing(prevData, data);
  else if (options.structuralSharing !== false) {
    if (true) try {
      return replaceEqualDeep(prevData, data);
    } catch (error) {
      console.error(`Structural sharing requires data to be JSON serializable. To fix this, turn off structuralSharing or return JSON-serializable data from your queryFn. [${options.queryHash}]: ${error}`);
      throw error;
    }
    return replaceEqualDeep(prevData, data);
  }
  return data;
}
function addToEnd(items, item, max = 0) {
  const newItems = [...items, item];
  return max && newItems.length > max ? newItems.slice(1) : newItems;
}
function addToStart(items, item, max = 0) {
  const newItems = [item, ...items];
  return max && newItems.length > max ? newItems.slice(0, -1) : newItems;
}
var skipToken = Symbol();
function ensureQueryFn(options, fetchOptions) {
  if (true) {
    if (options.queryFn === skipToken) console.error(`Attempted to invoke queryFn when set to skipToken. This is likely a configuration error. Query hash: '${options.queryHash}'`);
  }
  if (!options.queryFn && fetchOptions?.initialPromise) return () => fetchOptions.initialPromise;
  if (!options.queryFn || options.queryFn === skipToken) return () => Promise.reject(/* @__PURE__ */ new Error(`Missing queryFn: '${options.queryHash}'`));
  return options.queryFn;
}
function shouldThrowError(throwOnError, params) {
  if (typeof throwOnError === "function") return throwOnError(...params);
  return !!throwOnError;
}
function addConsumeAwareSignal(object, getSignal, onCancelled) {
  let consumed = false;
  let signal;
  Object.defineProperty(object, "signal", {
    enumerable: true,
    get: () => {
      signal ??= getSignal();
      if (consumed) return signal;
      consumed = true;
      if (signal.aborted) onCancelled();
      else signal.addEventListener("abort", onCancelled, { once: true });
      return signal;
    }
  });
  return object;
}

// node_modules/@tanstack/query-core/build/modern/environmentManager.js
var isServerFn = () => isServer;
var isServer2 = () => isServerFn();

// node_modules/@tanstack/query-core/build/modern/subscribable.js
var Subscribable = class {
  constructor() {
    this.listeners = /* @__PURE__ */ new Set();
    this.subscribe = this.subscribe.bind(this);
  }
  subscribe(listener) {
    this.listeners.add(listener);
    this.onSubscribe();
    return () => {
      this.listeners.delete(listener);
      this.onUnsubscribe();
    };
  }
  hasListeners() {
    return this.listeners.size > 0;
  }
  onSubscribe() {
  }
  onUnsubscribe() {
  }
};

// node_modules/@tanstack/query-core/build/modern/focusManager.js
var FocusManager = class extends Subscribable {
  #focused;
  #cleanup;
  #setup;
  constructor() {
    super();
    this.#setup = (onFocus) => {
      if (typeof window !== "undefined" && window.addEventListener) {
        const listener = () => onFocus();
        window.addEventListener("visibilitychange", listener, false);
        return () => {
          window.removeEventListener("visibilitychange", listener);
        };
      }
    };
  }
  onSubscribe() {
    if (!this.#cleanup) this.setEventListener(this.#setup);
  }
  onUnsubscribe() {
    if (!this.hasListeners()) {
      this.#cleanup?.();
      this.#cleanup = void 0;
    }
  }
  setEventListener(setup) {
    this.#setup = setup;
    this.#cleanup?.();
    this.#cleanup = setup((focused) => {
      if (typeof focused === "boolean") this.setFocused(focused);
      else this.onFocus();
    });
  }
  setFocused(focused) {
    if (this.#focused !== focused) {
      this.#focused = focused;
      this.onFocus();
    }
  }
  onFocus() {
    const isFocused = this.isFocused();
    this.listeners.forEach((listener) => {
      listener(isFocused);
    });
  }
  isFocused() {
    if (typeof this.#focused === "boolean") return this.#focused;
    return globalThis.document?.visibilityState !== "hidden";
  }
};
var focusManager = new FocusManager();

// node_modules/@tanstack/query-core/build/modern/notifyManager.js
var defaultScheduler = systemSetTimeoutZero;
function createNotifyManager() {
  let queue = [];
  let transactions = 0;
  let notifyFn = (callback) => {
    callback();
  };
  let batchNotifyFn = (callback) => {
    callback();
  };
  let scheduleFn = defaultScheduler;
  const schedule = (callback) => {
    if (transactions) queue.push(callback);
    else scheduleFn(() => {
      notifyFn(callback);
    });
  };
  const flush = () => {
    const originalQueue = queue;
    queue = [];
    if (originalQueue.length) scheduleFn(() => {
      batchNotifyFn(() => {
        originalQueue.forEach((callback) => {
          notifyFn(callback);
        });
      });
    });
  };
  return {
    batch: (callback) => {
      let result;
      transactions++;
      try {
        result = callback();
      } finally {
        transactions--;
        if (!transactions) flush();
      }
      return result;
    },
    /**
    * All calls to the wrapped function will be batched.
    */
    batchCalls: (callback) => {
      return (...args) => {
        schedule(() => {
          callback(...args);
        });
      };
    },
    schedule,
    /**
    * Use this method to set a custom notify function.
    * This can be used to for example wrap notifications with `React.act` while running tests.
    */
    setNotifyFunction: (fn) => {
      notifyFn = fn;
    },
    /**
    * Use this method to set a custom function to batch notifications together into a single tick.
    * By default React Query will use the batch function provided by ReactDOM or React Native.
    */
    setBatchNotifyFunction: (fn) => {
      batchNotifyFn = fn;
    },
    setScheduler: (fn) => {
      scheduleFn = fn;
    }
  };
}
var notifyManager = createNotifyManager();

// node_modules/@tanstack/query-core/build/modern/onlineManager.js
var OnlineManager = class extends Subscribable {
  #online = true;
  #cleanup;
  #setup;
  constructor() {
    super();
    this.#setup = (onOnline) => {
      if (typeof window !== "undefined" && window.addEventListener) {
        const onlineListener = () => onOnline(true);
        const offlineListener = () => onOnline(false);
        window.addEventListener("online", onlineListener, false);
        window.addEventListener("offline", offlineListener, false);
        return () => {
          window.removeEventListener("online", onlineListener);
          window.removeEventListener("offline", offlineListener);
        };
      }
    };
  }
  onSubscribe() {
    if (!this.#cleanup) this.setEventListener(this.#setup);
  }
  onUnsubscribe() {
    if (!this.hasListeners()) {
      this.#cleanup?.();
      this.#cleanup = void 0;
    }
  }
  setEventListener(setup) {
    this.#setup = setup;
    this.#cleanup?.();
    this.#cleanup = setup(this.setOnline.bind(this));
  }
  setOnline(online) {
    if (this.#online !== online) {
      this.#online = online;
      this.listeners.forEach((listener) => {
        listener(online);
      });
    }
  }
  isOnline() {
    return this.#online;
  }
};
var onlineManager = new OnlineManager();

// node_modules/@tanstack/query-core/build/modern/retryer.js
function defaultRetryDelay(failureCount) {
  return Math.min(1e3 * 2 ** failureCount, 3e4);
}
function canFetch(networkMode) {
  return (networkMode ?? "online") === "online" ? onlineManager.isOnline() : true;
}
var CancelledError = class extends Error {
  constructor(options) {
    super("CancelledError");
    this.revert = options?.revert;
    this.silent = options?.silent;
  }
};
function createRetryer(config) {
  let isRetryCancelled = false;
  let failureCount = 0;
  let continueFn;
  let status = "pending";
  let promiseResolve;
  let promiseReject;
  const promise = new Promise((resolve2, reject2) => {
    promiseResolve = resolve2;
    promiseReject = reject2;
  });
  promise.catch(noop);
  const isResolved = () => status !== "pending";
  const cancel = (cancelOptions) => {
    if (!isResolved()) {
      const error = new CancelledError(cancelOptions);
      reject(error);
      config.onCancel?.(error);
    }
  };
  const cancelRetry = () => {
    isRetryCancelled = true;
  };
  const continueRetry = () => {
    isRetryCancelled = false;
  };
  const canContinue = () => focusManager.isFocused() && (config.networkMode === "always" || onlineManager.isOnline()) && config.canRun();
  const canStart = () => canFetch(config.networkMode) && config.canRun();
  const resolve = (value) => {
    if (!isResolved()) {
      continueFn?.();
      status = "resolved";
      promiseResolve(value);
    }
  };
  const reject = (value) => {
    if (!isResolved()) {
      continueFn?.();
      status = "rejected";
      promiseReject(value);
    }
  };
  const pause = () => {
    return new Promise((continueResolve) => {
      continueFn = (value) => {
        if (isResolved() || canContinue()) continueResolve(value);
      };
      config.onPause?.();
    }).then(() => {
      continueFn = void 0;
      if (!isResolved()) config.onContinue?.();
    });
  };
  const run = () => {
    if (isResolved()) return;
    let promiseOrValue;
    const initialPromise = failureCount === 0 ? config.initialPromise : void 0;
    try {
      promiseOrValue = initialPromise ?? config.fn();
    } catch (error) {
      promiseOrValue = Promise.reject(error);
    }
    Promise.resolve(promiseOrValue).then(resolve).catch((error) => {
      if (isResolved()) return;
      const retry = config.retry ?? (isServer2() ? 0 : 3);
      const retryDelay = config.retryDelay ?? defaultRetryDelay;
      const delay = typeof retryDelay === "function" ? retryDelay(failureCount, error) : retryDelay;
      const shouldRetry = retry === true || typeof retry === "number" && failureCount < retry || typeof retry === "function" && retry(failureCount, error);
      if (isRetryCancelled || !shouldRetry) {
        reject(error);
        return;
      }
      failureCount++;
      config.onFail?.(failureCount, error);
      sleep(delay).then(() => {
        return canContinue() ? void 0 : pause();
      }).then(() => {
        if (isRetryCancelled) reject(error);
        else run();
      });
    });
  };
  return {
    promise,
    status: () => status,
    cancel,
    continue: () => {
      continueFn?.();
      return promise;
    },
    cancelRetry,
    continueRetry,
    canStart,
    start: () => {
      if (canStart()) run();
      else pause().then(run);
      return promise;
    }
  };
}

// node_modules/@tanstack/query-core/build/modern/removable.js
var Removable = class {
  #gcTimeout;
  destroy() {
    this.clearGcTimeout();
  }
  scheduleGc() {
    this.clearGcTimeout();
    if (isValidTimeout(this.gcTime)) this.#gcTimeout = timeoutManager.setTimeout(() => {
      this.optionalRemove();
    }, this.gcTime);
  }
  updateGcTime(newGcTime) {
    this.gcTime = Math.max(this.gcTime || 0, newGcTime ?? (isServer2() ? Infinity : 3e5));
  }
  clearGcTimeout() {
    if (this.#gcTimeout !== void 0) {
      timeoutManager.clearTimeout(this.#gcTimeout);
      this.#gcTimeout = void 0;
    }
  }
};

// node_modules/@tanstack/query-core/build/modern/infiniteQueryBehavior.js
function infiniteQueryBehavior(pages) {
  return { onFetch: (context, query) => {
    const options = context.options;
    const direction = context.fetchOptions?.meta?.fetchMore?.direction;
    const oldPages = context.state.data?.pages || [];
    const oldPageParams = context.state.data?.pageParams || [];
    let result = {
      pages: [],
      pageParams: []
    };
    let currentPage = 0;
    const fetchFn = async () => {
      let cancelled = false;
      const addSignalProperty = (object) => {
        addConsumeAwareSignal(object, () => context.signal, () => cancelled = true);
      };
      const queryFn = ensureQueryFn(context.options, context.fetchOptions);
      const fetchPage = async (data, param, previous) => {
        if (cancelled) return Promise.reject(context.signal.reason);
        if (param == null && data.pages.length) return Promise.resolve(data);
        const createQueryFnContext = () => {
          const queryFnContext2 = {
            client: context.client,
            queryKey: context.queryKey,
            pageParam: param,
            direction: previous ? "backward" : "forward",
            meta: context.options.meta
          };
          addSignalProperty(queryFnContext2);
          return queryFnContext2;
        };
        const queryFnContext = createQueryFnContext();
        const page = await queryFn(queryFnContext);
        const { maxPages } = context.options;
        const addTo = previous ? addToStart : addToEnd;
        return {
          pages: addTo(data.pages, page, maxPages),
          pageParams: addTo(data.pageParams, param, maxPages)
        };
      };
      if (direction && oldPages.length) {
        const previous = direction === "backward";
        const pageParamFn = previous ? getPreviousPageParam : getNextPageParam;
        const oldData = {
          pages: oldPages,
          pageParams: oldPageParams
        };
        result = await fetchPage(oldData, pageParamFn(options, oldData), previous);
      } else {
        const remainingPages = pages ?? oldPages.length;
        do {
          const param = currentPage === 0 ? oldPageParams[0] ?? options.initialPageParam : getNextPageParam(options, result);
          if (currentPage > 0 && param == null) break;
          result = await fetchPage(result, param);
          currentPage++;
        } while (currentPage < remainingPages);
      }
      return result;
    };
    if (context.options.persister) context.fetchFn = () => {
      return context.options.persister?.(fetchFn, {
        client: context.client,
        queryKey: context.queryKey,
        meta: context.options.meta,
        signal: context.signal
      }, query);
    };
    else context.fetchFn = fetchFn;
  } };
}
function getNextPageParam(options, { pages, pageParams }) {
  const lastIndex = pages.length - 1;
  return pages.length > 0 ? options.getNextPageParam(pages[lastIndex], pages, pageParams[lastIndex], pageParams) : void 0;
}
function getPreviousPageParam(options, { pages, pageParams }) {
  return pages.length > 0 ? options.getPreviousPageParam?.(pages[0], pages, pageParams[0], pageParams) : void 0;
}

// node_modules/@tanstack/query-core/build/modern/query.js
var Query = class extends Removable {
  #queryType;
  #initialState;
  #revertState;
  #cache;
  #client;
  #retryer;
  #defaultOptions;
  #abortSignalConsumed;
  constructor(config) {
    super();
    this.#abortSignalConsumed = false;
    this.#defaultOptions = config.defaultOptions;
    this.setOptions(config.options);
    this.observers = [];
    this.#client = config.client;
    this.#cache = this.#client.getQueryCache();
    this.queryKey = config.queryKey;
    this.queryHash = config.queryHash;
    this.#initialState = getDefaultState(this.options);
    this.state = config.state ?? this.#initialState;
    this.scheduleGc();
  }
  get meta() {
    return this.options.meta;
  }
  get queryType() {
    return this.#queryType;
  }
  get promise() {
    return this.#retryer?.promise;
  }
  setOptions(options) {
    this.options = {
      ...this.#defaultOptions,
      ...options
    };
    if (options?._type) this.#queryType = options._type;
    this.updateGcTime(this.options.gcTime);
    if (this.state && this.state.data === void 0) {
      const defaultState = getDefaultState(this.options);
      if (defaultState.data !== void 0) {
        this.setState(successState(defaultState.data, defaultState.dataUpdatedAt));
        this.#initialState = defaultState;
      }
    }
  }
  optionalRemove() {
    if (!this.observers.length && this.state.fetchStatus === "idle") this.#cache.remove(this);
  }
  setData(newData, options) {
    const data = replaceData(this.state.data, newData, this.options);
    this.#dispatch({
      data,
      type: "success",
      dataUpdatedAt: options?.updatedAt,
      manual: options?.manual
    });
    return data;
  }
  setState(state) {
    this.#dispatch({
      type: "setState",
      state
    });
  }
  cancel(options) {
    const promise = this.#retryer?.promise;
    this.#retryer?.cancel(options);
    return promise ? promise.then(noop).catch(noop) : Promise.resolve();
  }
  destroy() {
    super.destroy();
    this.cancel({ silent: true });
  }
  get resetState() {
    return this.#initialState;
  }
  reset() {
    this.destroy();
    this.setState(this.resetState);
  }
  isActive() {
    return this.observers.some((observer) => resolveQueryValue(observer.options.enabled, this) !== false);
  }
  isDisabled() {
    if (this.getObserversCount() > 0) return !this.isActive();
    return this.options.queryFn === skipToken || !this.isFetched();
  }
  isFetched() {
    return this.state.dataUpdateCount + this.state.errorUpdateCount > 0;
  }
  isStatic() {
    if (this.getObserversCount() > 0) return this.observers.some((observer) => resolveQueryValue(observer.options.staleTime, this) === "static");
    return false;
  }
  isStale() {
    if (this.getObserversCount() > 0) return this.observers.some((observer) => observer.getCurrentResult().isStale);
    return this.state.data === void 0 || this.state.isInvalidated;
  }
  isStaleByTime(staleTime = 0) {
    if (this.state.data === void 0) return true;
    if (staleTime === "static") return false;
    if (this.state.isInvalidated) return true;
    return !timeUntilStale(this.state.dataUpdatedAt, staleTime);
  }
  onFocus() {
    this.observers.find((x) => x.shouldFetchOnWindowFocus())?.refetch({ cancelRefetch: false });
    this.#retryer?.continue();
  }
  onOnline() {
    this.observers.find((x) => x.shouldFetchOnReconnect())?.refetch({ cancelRefetch: false });
    this.#retryer?.continue();
  }
  addObserver(observer) {
    if (!this.observers.includes(observer)) {
      this.observers.push(observer);
      this.clearGcTimeout();
      this.#cache.notify({
        type: "observerAdded",
        query: this,
        observer
      });
    }
  }
  removeObserver(observer) {
    const index = this.observers.indexOf(observer);
    if (index !== -1) {
      this.observers.splice(index, 1);
      if (!this.observers.length) {
        if (this.#retryer) {
          if (this.#abortSignalConsumed || this.state.fetchStatus === "paused" && this.state.status === "pending") this.#retryer.cancel({ revert: true });
          else this.#retryer.cancelRetry();
        }
        this.scheduleGc();
      }
      this.#cache.notify({
        type: "observerRemoved",
        query: this,
        observer
      });
    }
  }
  getObserversCount() {
    return this.observers.length;
  }
  invalidate() {
    if (!this.state.isInvalidated) this.#dispatch({ type: "invalidate" });
  }
  async fetch(options, fetchOptions) {
    if (this.state.fetchStatus !== "idle" && this.#retryer?.status() !== "rejected") {
      if (this.state.data !== void 0 && fetchOptions?.cancelRefetch) this.cancel({ silent: true });
      else if (this.#retryer) {
        this.#retryer.continueRetry();
        return this.#retryer.promise;
      }
    }
    if (options) this.setOptions(options);
    if (!this.options.queryFn) {
      const observer = this.observers.find((x) => x.options.queryFn);
      if (observer) this.setOptions(observer.options);
    }
    if (true) {
      if (!Array.isArray(this.options.queryKey)) console.error(`As of v4, queryKey needs to be an Array. If you are using a string like 'repoData', please change it to an Array, e.g. ['repoData']`);
    }
    const abortController = new AbortController();
    const addSignalProperty = (object) => {
      Object.defineProperty(object, "signal", {
        enumerable: true,
        get: () => {
          this.#abortSignalConsumed = true;
          return abortController.signal;
        }
      });
    };
    const fetchFn = () => {
      const queryFn = ensureQueryFn(this.options, fetchOptions);
      const createQueryFnContext = () => {
        const queryFnContext2 = {
          client: this.#client,
          queryKey: this.queryKey,
          meta: this.meta
        };
        addSignalProperty(queryFnContext2);
        return queryFnContext2;
      };
      const queryFnContext = createQueryFnContext();
      this.#abortSignalConsumed = false;
      if (this.options.persister) return this.options.persister(queryFn, queryFnContext, this);
      return queryFn(queryFnContext);
    };
    const createFetchContext = () => {
      const context2 = {
        fetchOptions,
        options: this.options,
        queryKey: this.queryKey,
        client: this.#client,
        state: this.state,
        fetchFn
      };
      addSignalProperty(context2);
      return context2;
    };
    const context = createFetchContext();
    (this.#queryType === "infinite" ? infiniteQueryBehavior(this.options.pages) : this.options.behavior)?.onFetch(context, this);
    this.#revertState = this.state;
    if (this.state.fetchStatus === "idle" || this.state.fetchMeta !== context.fetchOptions?.meta) this.#dispatch({
      type: "fetch",
      meta: context.fetchOptions?.meta
    });
    const retryer = this.#retryer = createRetryer({
      initialPromise: fetchOptions?.initialPromise,
      fn: context.fetchFn,
      onCancel: (error) => {
        if (error instanceof CancelledError && error.revert) this.setState({
          ...this.#revertState,
          fetchStatus: "idle"
        });
        abortController.abort();
      },
      onFail: (failureCount, error) => {
        this.#dispatch({
          type: "failed",
          failureCount,
          error
        });
      },
      onPause: () => {
        this.#dispatch({ type: "pause" });
      },
      onContinue: () => {
        this.#dispatch({ type: "continue" });
      },
      retry: context.options.retry,
      retryDelay: context.options.retryDelay,
      networkMode: context.options.networkMode,
      canRun: () => true
    });
    try {
      const data = await retryer.start();
      if (data === void 0) {
        if (true) console.error(`Query data cannot be undefined. Please make sure to return a value other than undefined from your query function. Affected query key: ${this.queryHash}`);
        throw new Error(`${this.queryHash} data is undefined`);
      }
      this.setData(data);
      this.#cache.config.onSuccess?.(data, this);
      this.#cache.config.onSettled?.(data, this.state.error, this);
      return data;
    } catch (error) {
      if (error instanceof CancelledError) {
        if (error.silent) return this.#retryer.promise;
        else if (error.revert) {
          if (this.state.data === void 0) throw error;
          return this.state.data;
        }
      }
      this.#dispatch({
        type: "error",
        error
      });
      this.#cache.config.onError?.(error, this);
      this.#cache.config.onSettled?.(this.state.data, error, this);
      throw error;
    } finally {
      if (this.#retryer === retryer) this.#retryer = void 0;
      this.scheduleGc();
    }
  }
  #dispatch(action) {
    const reducer = (state) => {
      switch (action.type) {
        case "failed":
          return {
            ...state,
            fetchFailureCount: action.failureCount,
            fetchFailureReason: action.error
          };
        case "pause":
          return {
            ...state,
            fetchStatus: "paused"
          };
        case "continue":
          return {
            ...state,
            fetchStatus: "fetching"
          };
        case "fetch":
          return {
            ...state,
            ...fetchState(state.data, this.options),
            fetchMeta: action.meta ?? null
          };
        case "success":
          const newState = {
            ...state,
            ...successState(action.data, action.dataUpdatedAt),
            dataUpdateCount: state.dataUpdateCount + 1,
            ...!action.manual && {
              fetchStatus: "idle",
              fetchFailureCount: 0,
              fetchFailureReason: null
            }
          };
          this.#revertState = action.manual ? newState : void 0;
          return newState;
        case "error":
          const error = action.error;
          return {
            ...state,
            error,
            errorUpdateCount: state.errorUpdateCount + 1,
            errorUpdatedAt: Date.now(),
            fetchFailureCount: state.fetchFailureCount + 1,
            fetchFailureReason: error,
            fetchStatus: "idle",
            status: "error",
            isInvalidated: true
          };
        case "invalidate":
          return {
            ...state,
            isInvalidated: true
          };
        case "setState":
          return {
            ...state,
            ...action.state
          };
      }
    };
    this.state = reducer(this.state);
    notifyManager.batch(() => {
      this.observers.slice().forEach((observer) => {
        observer.onQueryUpdate();
      });
      this.#cache.notify({
        query: this,
        type: "updated",
        action
      });
    });
  }
};
function fetchState(data, options) {
  return {
    fetchFailureCount: 0,
    fetchFailureReason: null,
    fetchStatus: canFetch(options.networkMode) ? "fetching" : "paused",
    ...data === void 0 && {
      error: null,
      status: "pending"
    }
  };
}
function successState(data, dataUpdatedAt) {
  return {
    data,
    dataUpdatedAt: dataUpdatedAt ?? Date.now(),
    error: null,
    isInvalidated: false,
    status: "success"
  };
}
function getDefaultState(options) {
  const data = typeof options.initialData === "function" ? options.initialData() : options.initialData;
  const hasData = data !== void 0;
  const initialDataUpdatedAt = hasData ? typeof options.initialDataUpdatedAt === "function" ? options.initialDataUpdatedAt() : options.initialDataUpdatedAt : 0;
  return {
    data,
    dataUpdateCount: 0,
    dataUpdatedAt: hasData ? initialDataUpdatedAt ?? Date.now() : 0,
    error: null,
    errorUpdateCount: 0,
    errorUpdatedAt: 0,
    fetchFailureCount: 0,
    fetchFailureReason: null,
    fetchMeta: null,
    isInvalidated: false,
    status: hasData ? "success" : "pending",
    fetchStatus: "idle"
  };
}

// node_modules/@tanstack/query-core/build/modern/queryObserver.js
var QueryObserver = class extends Subscribable {
  #client;
  #currentQuery = void 0;
  #currentQueryInitialState = void 0;
  #currentResult = void 0;
  #currentResultState;
  #currentResultOptions;
  #selectError;
  #selectFn;
  #selectResult;
  #lastQueryWithDefinedData;
  #staleTimeoutId;
  #refetchIntervalId;
  #currentRefetchInterval;
  #trackedProps = /* @__PURE__ */ new Set();
  constructor(client, options) {
    super();
    this.options = options;
    this.#client = client;
    this.#selectError = null;
    this.bindMethods();
    this.setOptions(options);
  }
  bindMethods() {
    this.refetch = this.refetch.bind(this);
  }
  onSubscribe() {
    if (this.listeners.size === 1) {
      this.#currentQuery.addObserver(this);
      if (shouldFetchOnMount(this.#currentQuery, this.options)) this.#executeFetch();
      else this.updateResult();
      this.#updateTimers();
    }
  }
  onUnsubscribe() {
    if (!this.hasListeners()) this.destroy();
  }
  shouldFetchOnReconnect() {
    return shouldFetchOn(this.#currentQuery, this.options, this.options.refetchOnReconnect);
  }
  shouldFetchOnWindowFocus() {
    return shouldFetchOn(this.#currentQuery, this.options, this.options.refetchOnWindowFocus);
  }
  destroy() {
    this.listeners = /* @__PURE__ */ new Set();
    this.#clearStaleTimeout();
    this.#clearRefetchInterval();
    this.#currentQuery.removeObserver(this);
  }
  setOptions(options) {
    const prevOptions = this.options;
    const prevQuery = this.#currentQuery;
    this.options = this.#client.defaultQueryOptions(options);
    if (this.options.enabled !== void 0 && typeof this.options.enabled !== "boolean" && typeof this.options.enabled !== "function" && typeof resolveQueryValue(this.options.enabled, this.#currentQuery) !== "boolean") throw new Error("Expected enabled to be a boolean or a callback that returns a boolean");
    this.#updateQuery();
    this.#currentQuery.setOptions(this.options);
    if (prevOptions._defaulted && !shallowEqualObjects(this.options, prevOptions)) this.#client.getQueryCache().notify({
      type: "observerOptionsUpdated",
      query: this.#currentQuery,
      observer: this
    });
    const mounted = this.hasListeners();
    if (mounted && shouldFetchOptionally(this.#currentQuery, prevQuery, this.options, prevOptions)) this.#executeFetch();
    this.updateResult();
    if (mounted && (this.#currentQuery !== prevQuery || resolveQueryValue(this.options.enabled, this.#currentQuery) !== resolveQueryValue(prevOptions.enabled, this.#currentQuery) || resolveQueryValue(this.options.staleTime, this.#currentQuery) !== resolveQueryValue(prevOptions.staleTime, this.#currentQuery))) this.#updateStaleTimeout();
    const nextRefetchInterval = this.#computeRefetchInterval();
    if (mounted && (this.#currentQuery !== prevQuery || resolveQueryValue(this.options.enabled, this.#currentQuery) !== resolveQueryValue(prevOptions.enabled, this.#currentQuery) || nextRefetchInterval !== this.#currentRefetchInterval)) this.#updateRefetchInterval(nextRefetchInterval);
  }
  getOptimisticResult(options) {
    const query = this.#client.getQueryCache().build(this.#client, options);
    const result = this.createResult(query, options);
    if (!shallowEqualObjects(this.getCurrentResult(), result)) {
      this.#currentResult = result;
      this.#currentResultOptions = this.options;
      this.#currentResultState = this.#currentQuery.state;
    }
    return result;
  }
  getCurrentResult() {
    return this.#currentResult;
  }
  trackResult(result, onPropTracked) {
    return new Proxy(result, { get: (target, key) => {
      this.trackProp(key);
      onPropTracked?.(key);
      return Reflect.get(target, key);
    } });
  }
  trackProp(key) {
    this.#trackedProps.add(key);
  }
  getCurrentQuery() {
    return this.#currentQuery;
  }
  refetch({ ...options } = {}) {
    return this.fetch({ ...options });
  }
  fetchOptimistic(options) {
    const defaultedOptions = this.#client.defaultQueryOptions(options);
    const query = this.#client.getQueryCache().build(this.#client, defaultedOptions);
    let unsubscribe = () => {
    };
    let resolveEarly;
    const cachePromise = new Promise((resolve) => {
      resolveEarly = resolve;
      unsubscribe = this.#client.getQueryCache().subscribe((event) => {
        if (event.type === "updated" && event.query.queryHash === query.queryHash && query.state.data !== void 0) {
          unsubscribe();
          resolve(this.createResult(query, defaultedOptions));
        }
      });
    });
    return Promise.race([query.fetch().then(() => {
      const result = this.createResult(query, defaultedOptions);
      resolveEarly?.(result);
      return result;
    }).finally(() => {
      unsubscribe();
    }), cachePromise]);
  }
  fetch(fetchOptions) {
    return this.#executeFetch({
      ...fetchOptions,
      cancelRefetch: fetchOptions.cancelRefetch ?? true
    }).then(() => {
      this.updateResult();
      return this.#currentResult;
    });
  }
  #executeFetch(fetchOptions) {
    this.#updateQuery();
    let promise = this.#currentQuery.fetch(this.options, fetchOptions);
    if (!fetchOptions?.throwOnError) promise = promise.catch(noop);
    return promise;
  }
  #shouldScheduleTimer(timeout) {
    return !isServer2() && resolveQueryValue(this.options.enabled, this.#currentQuery) !== false && isValidTimeout(timeout);
  }
  #updateStaleTimeout() {
    this.#clearStaleTimeout();
    const staleTime = resolveQueryValue(this.options.staleTime, this.#currentQuery);
    if (this.#currentResult.isStale || !this.#shouldScheduleTimer(staleTime)) return;
    const timeout = timeUntilStale(this.#currentResult.dataUpdatedAt, staleTime) + 1;
    this.#staleTimeoutId = timeoutManager.setTimeout(() => {
      if (!this.#currentResult.isStale) this.updateResult();
    }, timeout);
  }
  #computeRefetchInterval() {
    return (typeof this.options.refetchInterval === "function" ? this.options.refetchInterval(this.#currentQuery) : this.options.refetchInterval) ?? false;
  }
  #updateRefetchInterval(nextInterval) {
    this.#clearRefetchInterval();
    this.#currentRefetchInterval = nextInterval;
    if (this.#currentRefetchInterval === 0 || !this.#shouldScheduleTimer(this.#currentRefetchInterval)) return;
    this.#refetchIntervalId = timeoutManager.setInterval(() => {
      if (this.options.refetchIntervalInBackground || focusManager.isFocused()) this.#executeFetch();
    }, this.#currentRefetchInterval);
  }
  #updateTimers() {
    this.#updateStaleTimeout();
    this.#updateRefetchInterval(this.#computeRefetchInterval());
  }
  #clearStaleTimeout() {
    if (this.#staleTimeoutId !== void 0) {
      timeoutManager.clearTimeout(this.#staleTimeoutId);
      this.#staleTimeoutId = void 0;
    }
  }
  #clearRefetchInterval() {
    if (this.#refetchIntervalId !== void 0) {
      timeoutManager.clearInterval(this.#refetchIntervalId);
      this.#refetchIntervalId = void 0;
    }
  }
  createResult(query, options) {
    const prevQuery = this.#currentQuery;
    const prevOptions = this.options;
    const prevResult = this.#currentResult;
    const prevResultState = this.#currentResultState;
    const prevResultOptions = this.#currentResultOptions;
    const queryInitialState = query !== prevQuery ? query.state : this.#currentQueryInitialState;
    const { state } = query;
    let newState = { ...state };
    let isPlaceholderData = false;
    let data;
    if (options._optimisticResults) {
      const mounted = this.hasListeners();
      const fetchOnMount = !mounted && shouldFetchOnMount(query, options);
      const fetchOptionally = mounted && shouldFetchOptionally(query, prevQuery, options, prevOptions);
      if (fetchOnMount || fetchOptionally) newState = {
        ...newState,
        ...fetchState(state.data, query.options)
      };
      if (options._optimisticResults === "isRestoring") newState.fetchStatus = "idle";
    }
    let { error, errorUpdatedAt, status } = newState;
    data = newState.data;
    let skipSelect = false;
    if (options.placeholderData !== void 0 && data === void 0 && status === "pending") {
      let placeholderData;
      if (prevResult?.isPlaceholderData && options.placeholderData === prevResultOptions?.placeholderData) {
        placeholderData = prevResult.data;
        skipSelect = true;
      } else placeholderData = typeof options.placeholderData === "function" ? options.placeholderData(this.#lastQueryWithDefinedData?.state.data, this.#lastQueryWithDefinedData) : options.placeholderData;
      if (placeholderData !== void 0) {
        status = "success";
        data = replaceData(prevResult?.data, placeholderData, options);
        isPlaceholderData = true;
      }
    }
    if (options.select && data !== void 0 && !skipSelect) {
      if (prevResult && data === prevResultState?.data && options.select === this.#selectFn) data = this.#selectResult;
      else try {
        this.#selectFn = options.select;
        data = options.select(data);
        data = replaceData(prevResult?.data, data, options);
        this.#selectResult = data;
        this.#selectError = null;
      } catch (selectError) {
        this.#selectError = selectError;
      }
    } else if (data === void 0) this.#selectError = null;
    if (this.#selectError) {
      error = this.#selectError;
      data = this.#selectResult;
      errorUpdatedAt = Date.now();
      status = "error";
      isPlaceholderData = false;
    }
    const isFetching = newState.fetchStatus === "fetching";
    const isPending = status === "pending";
    const isError = status === "error";
    const isLoading = isPending && isFetching;
    const hasData = data !== void 0;
    return {
      status,
      fetchStatus: newState.fetchStatus,
      isPending,
      isSuccess: status === "success",
      isError,
      isInitialLoading: isLoading,
      isLoading,
      data,
      dataUpdatedAt: newState.dataUpdatedAt,
      error,
      errorUpdatedAt,
      failureCount: newState.fetchFailureCount,
      failureReason: newState.fetchFailureReason,
      errorUpdateCount: newState.errorUpdateCount,
      isFetched: query.isFetched(),
      isFetchedAfterMount: newState.dataUpdateCount > queryInitialState.dataUpdateCount || newState.errorUpdateCount > queryInitialState.errorUpdateCount,
      isFetching,
      isRefetching: isFetching && !isPending,
      isLoadingError: isError && !hasData,
      isPaused: newState.fetchStatus === "paused",
      isPlaceholderData,
      isRefetchError: isError && hasData,
      isStale: isStale(query, options),
      refetch: this.refetch,
      isEnabled: resolveQueryValue(options.enabled, query) !== false
    };
  }
  updateResult() {
    const prevResult = this.#currentResult;
    const nextResult = this.createResult(this.#currentQuery, this.options);
    this.#currentResultState = this.#currentQuery.state;
    this.#currentResultOptions = this.options;
    if (this.#currentResultState.data !== void 0) this.#lastQueryWithDefinedData = this.#currentQuery;
    if (shallowEqualObjects(nextResult, prevResult)) return;
    this.#currentResult = nextResult;
    const shouldNotifyListeners = () => {
      if (!prevResult) return true;
      const { notifyOnChangeProps } = this.options;
      const notifyOnChangePropsValue = typeof notifyOnChangeProps === "function" ? notifyOnChangeProps() : notifyOnChangeProps;
      if (notifyOnChangePropsValue === "all" || !notifyOnChangePropsValue && !this.#trackedProps.size) return true;
      const includedProps = new Set(notifyOnChangePropsValue ?? this.#trackedProps);
      if (this.options.throwOnError) includedProps.add("error");
      return Object.keys(this.#currentResult).some((key) => {
        const typedKey = key;
        return this.#currentResult[typedKey] !== prevResult[typedKey] && includedProps.has(typedKey);
      });
    };
    const notifyListeners = shouldNotifyListeners();
    notifyManager.batch(() => {
      if (notifyListeners) this.listeners.forEach((listener) => {
        listener(this.#currentResult);
      });
      this.#client.getQueryCache().notify({
        query: this.#currentQuery,
        type: "observerResultsUpdated"
      });
    });
  }
  #updateQuery() {
    const query = this.#client.getQueryCache().build(this.#client, this.options);
    if (query === this.#currentQuery) return;
    const prevQuery = this.#currentQuery;
    this.#currentQuery = query;
    this.#currentQueryInitialState = query.state;
    if (this.hasListeners()) {
      prevQuery?.removeObserver(this);
      query.addObserver(this);
    }
  }
  onQueryUpdate() {
    this.updateResult();
    if (this.hasListeners()) this.#updateTimers();
  }
};
function shouldLoadOnMount(query, options) {
  return resolveQueryValue(options.enabled, query) !== false && query.state.data === void 0 && !(query.state.status === "error" && resolveQueryValue(options.retryOnMount, query) === false);
}
function shouldFetchOnMount(query, options) {
  return shouldLoadOnMount(query, options) || query.state.data !== void 0 && shouldFetchOn(query, options, options.refetchOnMount);
}
function shouldFetchOn(query, options, field) {
  if (resolveQueryValue(options.enabled, query) !== false && resolveQueryValue(options.staleTime, query) !== "static") {
    const value = typeof field === "function" ? field(query) : field;
    return value === "always" || value !== false && isStale(query, options);
  }
  return false;
}
function shouldFetchOptionally(query, prevQuery, options, prevOptions) {
  return (query !== prevQuery || resolveQueryValue(prevOptions.enabled, query) === false) && (!options.suspense || query.state.status !== "error") && isStale(query, options);
}
function isStale(query, options) {
  return resolveQueryValue(options.enabled, query) !== false && query.isStaleByTime(resolveQueryValue(options.staleTime, query));
}

// node_modules/@tanstack/query-core/build/modern/mutation.js
var Mutation = class extends Removable {
  #client;
  #observers;
  #mutationCache;
  #retryer;
  constructor(config) {
    super();
    this.#client = config.client;
    this.mutationId = config.mutationId;
    this.#mutationCache = config.mutationCache;
    this.#observers = [];
    this.state = config.state || getDefaultState2();
    this.setOptions(config.options);
    this.scheduleGc();
  }
  setOptions(options) {
    this.options = options;
    this.updateGcTime(this.options.gcTime);
  }
  get meta() {
    return this.options.meta;
  }
  addObserver(observer) {
    if (!this.#observers.includes(observer)) {
      this.#observers.push(observer);
      this.clearGcTimeout();
      this.#mutationCache.notify({
        type: "observerAdded",
        mutation: this,
        observer
      });
    }
  }
  removeObserver(observer) {
    this.#observers = this.#observers.filter((x) => x !== observer);
    this.scheduleGc();
    this.#mutationCache.notify({
      type: "observerRemoved",
      mutation: this,
      observer
    });
  }
  optionalRemove() {
    if (!this.#observers.length) {
      if (this.state.status === "pending") this.scheduleGc();
      else this.#mutationCache.remove(this);
    }
  }
  continue() {
    return this.#retryer?.continue() ?? (this.state.status === "pending" ? this.execute(this.state.variables) : Promise.resolve());
  }
  async execute(variables) {
    const onContinue = () => {
      this.#dispatch({ type: "continue" });
    };
    const mutationFnContext = {
      client: this.#client,
      meta: this.options.meta,
      mutationKey: this.options.mutationKey
    };
    const retryer = this.#retryer = createRetryer({
      fn: () => {
        if (!this.options.mutationFn) return Promise.reject(/* @__PURE__ */ new Error("No mutationFn found"));
        return this.options.mutationFn(variables, mutationFnContext);
      },
      onFail: (failureCount, error) => {
        this.#dispatch({
          type: "failed",
          failureCount,
          error
        });
      },
      onPause: () => {
        this.#dispatch({ type: "pause" });
      },
      onContinue,
      retry: this.options.retry ?? 0,
      retryDelay: this.options.retryDelay,
      networkMode: this.options.networkMode,
      canRun: () => this.#mutationCache.canRun(this)
    });
    const restored = this.state.status === "pending";
    const isPaused = !retryer.canStart();
    try {
      if (restored) onContinue();
      else {
        this.#dispatch({
          type: "pending",
          variables,
          isPaused
        });
        if (this.#mutationCache.config.onMutate) await this.#mutationCache.config.onMutate(variables, this, mutationFnContext);
        const context = await this.options.onMutate?.(variables, mutationFnContext);
        if (context !== this.state.context) this.#dispatch({
          type: "pending",
          context,
          variables,
          isPaused
        });
      }
      const data = await retryer.start();
      await this.#mutationCache.config.onSuccess?.(data, variables, this.state.context, this, mutationFnContext);
      await this.options.onSuccess?.(data, variables, this.state.context, mutationFnContext);
      await this.#mutationCache.config.onSettled?.(data, null, this.state.variables, this.state.context, this, mutationFnContext);
      await this.options.onSettled?.(data, null, variables, this.state.context, mutationFnContext);
      this.#dispatch({
        type: "success",
        data
      });
      return data;
    } catch (error) {
      try {
        await this.#mutationCache.config.onError?.(error, variables, this.state.context, this, mutationFnContext);
      } catch (e) {
        Promise.reject(e);
      }
      try {
        await this.options.onError?.(error, variables, this.state.context, mutationFnContext);
      } catch (e) {
        Promise.reject(e);
      }
      try {
        await this.#mutationCache.config.onSettled?.(void 0, error, this.state.variables, this.state.context, this, mutationFnContext);
      } catch (e) {
        Promise.reject(e);
      }
      try {
        await this.options.onSettled?.(void 0, error, variables, this.state.context, mutationFnContext);
      } catch (e) {
        Promise.reject(e);
      }
      this.#dispatch({
        type: "error",
        error
      });
      throw error;
    } finally {
      if (this.#retryer === retryer) this.#retryer = void 0;
      this.#mutationCache.runNext(this);
    }
  }
  #dispatch(action) {
    const reducer = (state) => {
      switch (action.type) {
        case "failed":
          return {
            ...state,
            failureCount: action.failureCount,
            failureReason: action.error
          };
        case "pause":
          return {
            ...state,
            isPaused: true
          };
        case "continue":
          return {
            ...state,
            isPaused: false
          };
        case "pending":
          return {
            ...state,
            context: action.context,
            data: void 0,
            failureCount: 0,
            failureReason: null,
            error: null,
            isPaused: action.isPaused,
            status: "pending",
            variables: action.variables,
            submittedAt: Date.now()
          };
        case "success":
          return {
            ...state,
            data: action.data,
            failureCount: 0,
            failureReason: null,
            error: null,
            status: "success",
            isPaused: false
          };
        case "error":
          return {
            ...state,
            data: void 0,
            error: action.error,
            failureCount: state.failureCount + 1,
            failureReason: action.error,
            isPaused: false,
            status: "error"
          };
      }
    };
    this.state = reducer(this.state);
    notifyManager.batch(() => {
      this.#observers.forEach((observer) => {
        observer.onMutationUpdate(action);
      });
      this.#mutationCache.notify({
        mutation: this,
        type: "updated",
        action
      });
    });
  }
};
function getDefaultState2() {
  return {
    context: void 0,
    data: void 0,
    error: null,
    failureCount: 0,
    failureReason: null,
    isPaused: false,
    status: "idle",
    variables: void 0,
    submittedAt: 0
  };
}

// node_modules/@tanstack/query-core/build/modern/mutationCache.js
var MutationCache = class extends Subscribable {
  #mutations;
  #scopes;
  #mutationId;
  constructor(config = {}) {
    super();
    this.config = config;
    this.#mutations = /* @__PURE__ */ new Set();
    this.#scopes = /* @__PURE__ */ new Map();
    this.#mutationId = 0;
  }
  build(client, options, state) {
    const mutation = new Mutation({
      client,
      mutationCache: this,
      mutationId: ++this.#mutationId,
      options: client.defaultMutationOptions(options),
      state
    });
    this.add(mutation);
    return mutation;
  }
  add(mutation) {
    this.#mutations.add(mutation);
    const scope = scopeFor(mutation);
    if (typeof scope === "string") {
      const scopedMutations = this.#scopes.get(scope);
      if (scopedMutations) scopedMutations.push(mutation);
      else this.#scopes.set(scope, [mutation]);
    }
    this.notify({
      type: "added",
      mutation
    });
  }
  remove(mutation) {
    if (this.#mutations.delete(mutation)) {
      const scope = scopeFor(mutation);
      if (typeof scope === "string") {
        const scopedMutations = this.#scopes.get(scope);
        if (scopedMutations) {
          if (scopedMutations.length > 1) {
            const index = scopedMutations.indexOf(mutation);
            if (index !== -1) scopedMutations.splice(index, 1);
          } else if (scopedMutations[0] === mutation) this.#scopes.delete(scope);
        }
      }
    }
    this.notify({
      type: "removed",
      mutation
    });
  }
  canRun(mutation) {
    const scope = scopeFor(mutation);
    if (typeof scope === "string") {
      const firstPendingMutation = this.#scopes.get(scope)?.find((m) => m.state.status === "pending");
      return !firstPendingMutation || firstPendingMutation === mutation;
    } else return true;
  }
  runNext(mutation) {
    const scope = scopeFor(mutation);
    if (typeof scope === "string") return this.#scopes.get(scope)?.find((m) => m !== mutation && m.state.isPaused)?.continue() ?? Promise.resolve();
    else return Promise.resolve();
  }
  clear() {
    notifyManager.batch(() => {
      this.#mutations.forEach((mutation) => {
        this.notify({
          type: "removed",
          mutation
        });
      });
      this.#mutations.clear();
      this.#scopes.clear();
    });
  }
  getAll() {
    return Array.from(this.#mutations);
  }
  find(filters) {
    const defaultedFilters = {
      exact: true,
      ...filters
    };
    return this.getAll().find((mutation) => matchMutation(defaultedFilters, mutation));
  }
  findAll(filters = {}) {
    return this.getAll().filter((mutation) => matchMutation(filters, mutation));
  }
  notify(event) {
    notifyManager.batch(() => {
      this.listeners.forEach((listener) => {
        listener(event);
      });
    });
  }
  resumePausedMutations() {
    const pausedMutations = this.getAll().filter((x) => x.state.isPaused);
    return notifyManager.batch(() => Promise.all(pausedMutations.map((mutation) => mutation.continue().catch(noop))));
  }
};
function scopeFor(mutation) {
  return mutation.options.scope?.id;
}

// node_modules/@tanstack/query-core/build/modern/mutationObserver.js
var MutationObserver = class extends Subscribable {
  #client;
  #currentResult = void 0;
  #currentMutation;
  #mutateOptions;
  constructor(client, options) {
    super();
    this.#client = client;
    this.setOptions(options);
    this.bindMethods();
    this.#updateResult();
  }
  bindMethods() {
    this.mutate = this.mutate.bind(this);
    this.reset = this.reset.bind(this);
  }
  setOptions(options) {
    const prevOptions = this.options;
    this.options = this.#client.defaultMutationOptions(options);
    if (!shallowEqualObjects(this.options, prevOptions)) this.#client.getMutationCache().notify({
      type: "observerOptionsUpdated",
      mutation: this.#currentMutation,
      observer: this
    });
    if (prevOptions?.mutationKey && this.options.mutationKey && hashKey(prevOptions.mutationKey) !== hashKey(this.options.mutationKey)) this.reset();
    else if (this.#currentMutation?.state.status === "pending") this.#currentMutation.setOptions(this.options);
  }
  onSubscribe() {
    if (this.listeners.size === 1 && this.#currentMutation) {
      this.#currentMutation.addObserver(this);
      this.#updateResult();
    }
  }
  onUnsubscribe() {
    if (!this.hasListeners()) this.#currentMutation?.removeObserver(this);
  }
  onMutationUpdate(action) {
    this.#updateResult();
    this.#notify(action);
  }
  getCurrentResult() {
    return this.#currentResult;
  }
  reset() {
    this.#currentMutation?.removeObserver(this);
    this.#currentMutation = void 0;
    this.#updateResult();
    this.#notify();
  }
  mutate(variables, options) {
    this.#mutateOptions = options;
    this.#currentMutation?.removeObserver(this);
    this.#currentMutation = this.#client.getMutationCache().build(this.#client, this.options);
    this.#currentMutation.addObserver(this);
    return this.#currentMutation.execute(variables);
  }
  #updateResult() {
    const state = this.#currentMutation?.state ?? getDefaultState2();
    this.#currentResult = {
      ...state,
      isPending: state.status === "pending",
      isSuccess: state.status === "success",
      isError: state.status === "error",
      isIdle: state.status === "idle",
      mutate: this.mutate,
      reset: this.reset
    };
  }
  #notify(action) {
    notifyManager.batch(() => {
      if (this.#mutateOptions && this.hasListeners()) {
        const variables = this.#currentResult.variables;
        const onMutateResult = this.#currentResult.context;
        const context = {
          client: this.#client,
          meta: this.options.meta,
          mutationKey: this.options.mutationKey
        };
        if (action?.type === "success") {
          try {
            this.#mutateOptions.onSuccess?.(action.data, variables, onMutateResult, context);
          } catch (e) {
            Promise.reject(e);
          }
          try {
            this.#mutateOptions.onSettled?.(action.data, null, variables, onMutateResult, context);
          } catch (e) {
            Promise.reject(e);
          }
        } else if (action?.type === "error") {
          try {
            this.#mutateOptions.onError?.(action.error, variables, onMutateResult, context);
          } catch (e) {
            Promise.reject(e);
          }
          try {
            this.#mutateOptions.onSettled?.(void 0, action.error, variables, onMutateResult, context);
          } catch (e) {
            Promise.reject(e);
          }
        }
      }
      this.listeners.forEach((listener) => {
        listener(this.#currentResult);
      });
    });
  }
};

// node_modules/@tanstack/query-core/build/modern/queryCache.js
var QueryCache = class extends Subscribable {
  #queries;
  constructor(config = {}) {
    super();
    this.config = config;
    this.#queries = /* @__PURE__ */ new Map();
  }
  build(client, options, state) {
    const queryKey = options.queryKey;
    const queryHash = options.queryHash ?? hashQueryKeyByOptions(queryKey, options);
    let query = this.get(queryHash);
    if (!query) {
      query = new Query({
        client,
        queryKey,
        queryHash,
        options: client.defaultQueryOptions(options),
        state,
        defaultOptions: client.getQueryDefaults(queryKey)
      });
      this.add(query);
    }
    return query;
  }
  add(query) {
    if (!this.#queries.has(query.queryHash)) {
      this.#queries.set(query.queryHash, query);
      this.notify({
        type: "added",
        query
      });
    }
  }
  remove(query) {
    const queryInMap = this.#queries.get(query.queryHash);
    if (queryInMap) {
      query.destroy();
      if (queryInMap === query) this.#queries.delete(query.queryHash);
      this.notify({
        type: "removed",
        query
      });
    }
  }
  clear() {
    notifyManager.batch(() => {
      this.getAll().forEach((query) => {
        this.remove(query);
      });
    });
  }
  get(queryHash) {
    return this.#queries.get(queryHash);
  }
  getAll() {
    return [...this.#queries.values()];
  }
  find(filters) {
    const defaultedFilters = {
      exact: true,
      ...filters
    };
    return this.getAll().find((query) => matchQuery(defaultedFilters, query));
  }
  findAll(filters = {}) {
    const queries = this.getAll();
    return Object.keys(filters).length > 0 ? queries.filter((query) => matchQuery(filters, query)) : queries;
  }
  notify(event) {
    notifyManager.batch(() => {
      this.listeners.forEach((listener) => {
        listener(event);
      });
    });
  }
  onFocus() {
    notifyManager.batch(() => {
      this.getAll().forEach((query) => {
        query.onFocus();
      });
    });
  }
  onOnline() {
    notifyManager.batch(() => {
      this.getAll().forEach((query) => {
        query.onOnline();
      });
    });
  }
};

// node_modules/@tanstack/query-core/build/modern/queryClient.js
var QueryClient = class {
  #queryCache;
  #mutationCache;
  #defaultOptions;
  #queryDefaults;
  #mutationDefaults;
  #mountCount;
  #unsubscribeFocus;
  #unsubscribeOnline;
  constructor(config = {}) {
    this.#queryCache = config.queryCache || new QueryCache();
    this.#mutationCache = config.mutationCache || new MutationCache();
    this.#defaultOptions = config.defaultOptions || {};
    this.#queryDefaults = /* @__PURE__ */ new Map();
    this.#mutationDefaults = /* @__PURE__ */ new Map();
    this.#mountCount = 0;
  }
  mount() {
    this.#mountCount++;
    if (this.#mountCount !== 1) return;
    this.#unsubscribeFocus = focusManager.subscribe(async (focused) => {
      if (focused) {
        await this.resumePausedMutations();
        this.#queryCache.onFocus();
      }
    });
    this.#unsubscribeOnline = onlineManager.subscribe(async (online) => {
      if (online) {
        await this.resumePausedMutations();
        this.#queryCache.onOnline();
      }
    });
  }
  unmount() {
    this.#mountCount--;
    if (this.#mountCount !== 0) return;
    this.#unsubscribeFocus?.();
    this.#unsubscribeFocus = void 0;
    this.#unsubscribeOnline?.();
    this.#unsubscribeOnline = void 0;
  }
  isFetching(filters) {
    return this.#queryCache.findAll({
      ...filters,
      fetchStatus: "fetching"
    }).length;
  }
  isMutating(filters) {
    return this.#mutationCache.findAll({
      ...filters,
      status: "pending"
    }).length;
  }
  /**
  * Imperative (non-reactive) way to retrieve data for a QueryKey.
  * Should only be used in callbacks or functions where reading the latest data is necessary, e.g. for optimistic updates.
  *
  * Hint: Do not use this function inside a component, because it won't receive updates.
  * Use `useQuery` to create a `QueryObserver` that subscribes to changes.
  */
  getQueryData(queryKey) {
    const options = this.defaultQueryOptions({ queryKey });
    return this.#queryCache.get(options.queryHash)?.state.data;
  }
  /**
  * @deprecated Use queryClient.query({ ...options, staleTime: 'static' }) instead. This method will be removed in the next major version.
  */
  ensureQueryData(options) {
    const defaultedOptions = this.defaultQueryOptions(options);
    const query = this.#queryCache.build(this, defaultedOptions);
    const cachedData = query.state.data;
    if (cachedData === void 0) return this.fetchQuery(options);
    if (options.revalidateIfStale && query.isStaleByTime(resolveQueryValue(defaultedOptions.staleTime, query))) this.prefetchQuery(defaultedOptions);
    return Promise.resolve(cachedData);
  }
  getQueriesData(filters) {
    return this.#queryCache.findAll(filters).map(({ queryKey, state }) => {
      return [queryKey, state.data];
    });
  }
  setQueryData(queryKey, updater, options) {
    const defaultedOptions = this.defaultQueryOptions({ queryKey });
    const prevData = this.#queryCache.get(defaultedOptions.queryHash)?.state.data;
    const data = functionalUpdate(updater, prevData);
    if (data === void 0) return;
    return this.#queryCache.build(this, defaultedOptions).setData(data, {
      ...options,
      manual: true
    });
  }
  setQueriesData(filters, updater, options) {
    return notifyManager.batch(() => this.#queryCache.findAll(filters).map(({ queryKey }) => [queryKey, this.setQueryData(queryKey, updater, options)]));
  }
  getQueryState(queryKey) {
    const options = this.defaultQueryOptions({ queryKey });
    return this.#queryCache.get(options.queryHash)?.state;
  }
  removeQueries(filters) {
    const queryCache = this.#queryCache;
    notifyManager.batch(() => {
      queryCache.findAll(filters).forEach((query) => {
        queryCache.remove(query);
      });
    });
  }
  resetQueries(filters, options) {
    const queryCache = this.#queryCache;
    return notifyManager.batch(() => {
      const matched = queryCache.findAll(filters);
      const queriesToRefetch = new Set(matched);
      matched.forEach((query) => {
        query.reset();
      });
      return this.refetchQueries({
        type: "active",
        predicate: (query) => queriesToRefetch.has(query)
      }, options);
    });
  }
  cancelQueries(filters, cancelOptions = {}) {
    const defaultedCancelOptions = {
      revert: true,
      ...cancelOptions
    };
    const promises = notifyManager.batch(() => this.#queryCache.findAll(filters).map((query) => query.cancel(defaultedCancelOptions)));
    return Promise.all(promises).then(noop).catch(noop);
  }
  invalidateQueries(filters, options = {}) {
    return notifyManager.batch(() => {
      this.#queryCache.findAll(filters).forEach((query) => {
        query.invalidate();
      });
      if (filters?.refetchType === "none") return Promise.resolve();
      return this.refetchQueries({
        ...filters,
        type: filters?.refetchType ?? filters?.type ?? "active"
      }, options);
    });
  }
  refetchQueries(filters, options = {}) {
    const fetchOptions = {
      ...options,
      cancelRefetch: options.cancelRefetch ?? true
    };
    const promises = notifyManager.batch(() => this.#queryCache.findAll(filters).filter((query) => !query.isDisabled() && !query.isStatic()).map((query) => {
      let promise = query.fetch(void 0, fetchOptions);
      if (!fetchOptions.throwOnError) promise = promise.catch(noop);
      return query.state.fetchStatus === "paused" ? Promise.resolve() : promise;
    }));
    return Promise.all(promises).then(noop);
  }
  async query(options) {
    const defaultedOptions = this.defaultQueryOptions(options);
    if (defaultedOptions.retry === void 0) defaultedOptions.retry = false;
    const query = this.#queryCache.build(this, defaultedOptions);
    const queryData = query.isStaleByTime(resolveQueryValue(defaultedOptions.staleTime, query)) ? await query.fetch(defaultedOptions) : query.state.data;
    const select = defaultedOptions.select;
    if (select) return select(queryData);
    return queryData;
  }
  /**
  * @deprecated Use queryClient.query(options) instead. This method will be removed in the next major version.
  */
  fetchQuery(options) {
    const defaultedOptions = this.defaultQueryOptions(options);
    if (defaultedOptions.retry === void 0) defaultedOptions.retry = false;
    const query = this.#queryCache.build(this, defaultedOptions);
    return query.isStaleByTime(resolveQueryValue(defaultedOptions.staleTime, query)) ? query.fetch(defaultedOptions) : Promise.resolve(query.state.data);
  }
  /**
  * @deprecated Use queryClient.query(options) instead. You can swallow errors with `.catch(noop)`. This method will be removed in the next major version.
  */
  prefetchQuery(options) {
    return this.fetchQuery(options).then(noop).catch(noop);
  }
  infiniteQuery(options) {
    options._type = "infinite";
    return this.query(options);
  }
  /**
  * @deprecated Use queryClient.infiniteQuery(options) instead. This method will be removed in the next major version.
  */
  fetchInfiniteQuery(options) {
    options._type = "infinite";
    return this.fetchQuery(options);
  }
  /**
  * @deprecated Use queryClient.infiniteQuery(options) instead. You can swallow errors with `.catch(noop)`. This method will be removed in the next major version.
  */
  prefetchInfiniteQuery(options) {
    return this.fetchInfiniteQuery(options).then(noop).catch(noop);
  }
  /**
  * @deprecated Use queryClient.infiniteQuery({ ...options, staleTime: 'static' }) instead. This method will be removed in the next major version.
  */
  ensureInfiniteQueryData(options) {
    options._type = "infinite";
    return this.ensureQueryData(options);
  }
  resumePausedMutations() {
    if (onlineManager.isOnline()) return this.#mutationCache.resumePausedMutations();
    return Promise.resolve();
  }
  getQueryCache() {
    return this.#queryCache;
  }
  getMutationCache() {
    return this.#mutationCache;
  }
  getDefaultOptions() {
    return this.#defaultOptions;
  }
  setDefaultOptions(options) {
    this.#defaultOptions = options;
  }
  setQueryDefaults(queryKey, options) {
    this.#queryDefaults.set(hashKey(queryKey), {
      queryKey,
      defaultOptions: options
    });
  }
  getQueryDefaults(queryKey) {
    const defaults = [...this.#queryDefaults.values()];
    const result = {};
    defaults.forEach((queryDefault) => {
      if (partialMatchKey(queryKey, queryDefault.queryKey)) Object.assign(result, queryDefault.defaultOptions);
    });
    return result;
  }
  setMutationDefaults(mutationKey, options) {
    this.#mutationDefaults.set(hashKey(mutationKey), {
      mutationKey,
      defaultOptions: options
    });
  }
  getMutationDefaults(mutationKey) {
    const defaults = [...this.#mutationDefaults.values()];
    const result = {};
    defaults.forEach((queryDefault) => {
      if (partialMatchKey(mutationKey, queryDefault.mutationKey)) Object.assign(result, queryDefault.defaultOptions);
    });
    return result;
  }
  defaultQueryOptions(options) {
    if (options._defaulted) return options;
    const defaultedOptions = {
      ...this.#defaultOptions.queries,
      ...this.getQueryDefaults(options.queryKey),
      ...options,
      _defaulted: true
    };
    if (!defaultedOptions.queryHash) defaultedOptions.queryHash = hashQueryKeyByOptions(defaultedOptions.queryKey, defaultedOptions);
    if (defaultedOptions.refetchOnReconnect === void 0) defaultedOptions.refetchOnReconnect = defaultedOptions.networkMode !== "always";
    if (defaultedOptions.throwOnError === void 0) defaultedOptions.throwOnError = !!defaultedOptions.suspense;
    if (!defaultedOptions.networkMode && defaultedOptions.persister) defaultedOptions.networkMode = "offlineFirst";
    if (defaultedOptions.queryFn === skipToken) defaultedOptions.enabled = false;
    return defaultedOptions;
  }
  defaultMutationOptions(options) {
    if (options?._defaulted) return options;
    return {
      ...this.#defaultOptions.mutations,
      ...options?.mutationKey && this.getMutationDefaults(options.mutationKey),
      ...options,
      _defaulted: true
    };
  }
  clear() {
    this.#queryCache.clear();
    this.#mutationCache.clear();
  }
};

// node_modules/@tanstack/react-query/build/modern/IsRestoringProvider.js
var IsRestoringContext = createContext(false);
var useIsRestoring = () => useContext(IsRestoringContext);
var IsRestoringProvider = IsRestoringContext.Provider;

// node_modules/@tanstack/react-query/build/modern/QueryErrorResetBoundary.js
function createValue() {
  let isReset = false;
  return {
    clearReset: () => {
      isReset = false;
    },
    reset: () => {
      isReset = true;
    },
    isReset: () => {
      return isReset;
    }
  };
}
var QueryErrorResetBoundaryContext = createContext(createValue());
var useQueryErrorResetBoundary = () => useContext(QueryErrorResetBoundaryContext);

// node_modules/@tanstack/react-query/build/modern/errorBoundaryUtils.js
var ensurePreventErrorBoundaryRetry = (options, errorResetBoundary, query) => {
  const throwOnError = query?.state.error && typeof options.throwOnError === "function" ? shouldThrowError(options.throwOnError, [query.state.error, query]) : options.throwOnError;
  if (options.suspense || throwOnError) {
    if (!errorResetBoundary.isReset()) options.retryOnMount = false;
  }
};
var useClearResetErrorBoundary = (errorResetBoundary) => {
  useEffect(() => {
    errorResetBoundary.clearReset();
  }, [errorResetBoundary]);
};
var getHasError = ({ result, errorResetBoundary, throwOnError, query, suspense }) => {
  return result.isError && !errorResetBoundary.isReset() && !result.isFetching && query && (suspense && result.data === void 0 || shouldThrowError(throwOnError, [result.error, query]));
};

// node_modules/@tanstack/react-query/build/modern/suspense.js
var ensureSuspenseTimers = (defaultedOptions) => {
  if (defaultedOptions.suspense) {
    const MIN_SUSPENSE_TIME_MS = 1e3;
    const clamp = (value) => value === "static" ? value : Math.max(value ?? MIN_SUSPENSE_TIME_MS, MIN_SUSPENSE_TIME_MS);
    const originalStaleTime = defaultedOptions.staleTime;
    defaultedOptions.staleTime = typeof originalStaleTime === "function" ? (...args) => clamp(originalStaleTime(...args)) : clamp(originalStaleTime);
    if (typeof defaultedOptions.gcTime === "number") defaultedOptions.gcTime = Math.max(defaultedOptions.gcTime, MIN_SUSPENSE_TIME_MS);
  }
};
var shouldSuspend = (defaultedOptions, result) => defaultedOptions?.suspense && result.isPending;
var fetchOptimistic = (defaultedOptions, observer, errorResetBoundary) => observer.fetchOptimistic(defaultedOptions).catch(() => {
  errorResetBoundary.clearReset();
});

// node_modules/@tanstack/react-query/build/modern/useBaseQuery.js
function useBaseQuery(options, Observer, queryClient) {
  if (true) {
    if (typeof options !== "object" || Array.isArray(options)) throw new Error('Bad argument type. Starting with v5, only the "Object" form is allowed when calling query related functions. Please use the error stack to find the culprit call. More info here: https://tanstack.com/query/latest/docs/react/guides/migrating-to-v5#supports-a-single-signature-one-object');
  }
  const isRestoring = useIsRestoring();
  const errorResetBoundary = useQueryErrorResetBoundary();
  const client = useQueryClient(queryClient);
  const defaultedOptions = client.defaultQueryOptions(options);
  const query = client.getQueryCache().get(defaultedOptions.queryHash);
  if (true) {
    if (!defaultedOptions.queryFn) console.error(`[${defaultedOptions.queryHash}]: No queryFn was passed as an option, and no default queryFn was found. The queryFn parameter is only optional when using a default queryFn. More info here: https://tanstack.com/query/latest/docs/framework/react/guides/default-query-function`);
  }
  const subscribed = options.subscribed !== false;
  defaultedOptions._optimisticResults = isRestoring ? "isRestoring" : subscribed ? "optimistic" : void 0;
  ensureSuspenseTimers(defaultedOptions);
  ensurePreventErrorBoundaryRetry(defaultedOptions, errorResetBoundary, query);
  useClearResetErrorBoundary(errorResetBoundary);
  const [observer] = useState(() => new Observer(client, defaultedOptions));
  const result = observer.getOptimisticResult(defaultedOptions);
  const shouldSubscribe = !isRestoring && subscribed;
  useSyncExternalStore(useCallback((onStoreChange) => {
    const unsubscribe = shouldSubscribe ? observer.subscribe(notifyManager.batchCalls(onStoreChange)) : noop;
    observer.updateResult();
    return unsubscribe;
  }, [observer, shouldSubscribe]), () => observer.getCurrentResult(), () => observer.getCurrentResult());
  useEffect(() => {
    observer.setOptions(defaultedOptions);
  }, [defaultedOptions, observer]);
  if (shouldSuspend(defaultedOptions, result)) throw fetchOptimistic(defaultedOptions, observer, errorResetBoundary);
  if (getHasError({
    result,
    errorResetBoundary,
    throwOnError: defaultedOptions.throwOnError,
    query,
    suspense: defaultedOptions.suspense
  })) throw result.error;
  return !defaultedOptions.notifyOnChangeProps ? observer.trackResult(result) : result;
}

// node_modules/@tanstack/react-query/build/modern/useQuery.js
function useQuery(options, queryClient) {
  return useBaseQuery(options, QueryObserver, queryClient);
}

// node_modules/@tanstack/react-query/build/modern/useMutationState.js
function getResult(mutationCache, options) {
  return mutationCache.findAll(options.filters).map((mutation) => options.select ? options.select(mutation) : mutation.state);
}
function useMutationState(options = {}, queryClient) {
  const mutationCache = useQueryClient(queryClient).getMutationCache();
  const optionsRef = useRef(options);
  const result = useRef(null);
  if (result.current === null) result.current = getResult(mutationCache, options);
  useEffect(() => {
    optionsRef.current = options;
  });
  return useSyncExternalStore(useCallback((onStoreChange) => mutationCache.subscribe(() => {
    const nextResult = replaceEqualDeep(result.current, getResult(mutationCache, optionsRef.current));
    if (result.current !== nextResult) {
      result.current = nextResult;
      notifyManager.schedule(onStoreChange);
    }
  }), [mutationCache]), () => result.current, () => result.current);
}

// node_modules/@tanstack/react-query/build/modern/useMutation.js
function useMutation(options, queryClient) {
  const client = useQueryClient(queryClient);
  const [observer] = useState(() => new MutationObserver(client, options));
  useEffect(() => {
    observer.setOptions(options);
  }, [observer, options]);
  const result = useSyncExternalStore(useCallback((onStoreChange) => observer.subscribe(notifyManager.batchCalls(onStoreChange)), [observer]), () => observer.getCurrentResult(), () => observer.getCurrentResult());
  const mutate = useCallback((...args) => {
    observer.mutate(args[0], args[1]).catch(noop);
  }, [observer]);
  if (result.error && shouldThrowError(observer.options.throwOnError, [result.error])) throw result.error;
  return {
    ...result,
    mutate,
    mutateAsync: result.mutate
  };
}

// node_modules/lucide-react/dist/esm/shared/src/utils/mergeClasses.mjs
var mergeClasses = (...classes) => classes.filter((className, index, array) => {
  return Boolean(className) && className.trim() !== "" && array.indexOf(className) === index;
}).join(" ").trim();

// node_modules/lucide-react/dist/esm/shared/src/utils/toKebabCase.mjs
var toKebabCase = (string) => string.replace(/([a-z0-9])([A-Z])/g, "$1-$2").toLowerCase();

// node_modules/lucide-react/dist/esm/shared/src/utils/toCamelCase.mjs
var toCamelCase = (string) => string.replace(
  /^([A-Z])|[\s-_]+(\w)/g,
  (match, p1, p2) => p2 ? p2.toUpperCase() : p1.toLowerCase()
);

// node_modules/lucide-react/dist/esm/shared/src/utils/toPascalCase.mjs
var toPascalCase = (string) => {
  const camelCase = toCamelCase(string);
  return camelCase.charAt(0).toUpperCase() + camelCase.slice(1);
};

// node_modules/lucide-react/dist/esm/defaultAttributes.mjs
var defaultAttributes = {
  xmlns: "http://www.w3.org/2000/svg",
  width: 24,
  height: 24,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 2,
  strokeLinecap: "round",
  strokeLinejoin: "round"
};

// node_modules/lucide-react/dist/esm/shared/src/utils/hasA11yProp.mjs
var hasA11yProp = (props) => {
  for (const prop in props) {
    if (prop.startsWith("aria-") || prop === "role" || prop === "title") {
      return true;
    }
  }
  return false;
};

// node_modules/lucide-react/dist/esm/context.mjs
var LucideContext = createContext({});
var useLucideContext = () => useContext(LucideContext);

// node_modules/lucide-react/dist/esm/Icon.mjs
var Icon = forwardRef(
  ({ color, size, strokeWidth, absoluteStrokeWidth, className = "", children, iconNode, ...rest }, ref) => {
    const {
      size: contextSize = 24,
      strokeWidth: contextStrokeWidth = 2,
      absoluteStrokeWidth: contextAbsoluteStrokeWidth = false,
      color: contextColor = "currentColor",
      className: contextClass = ""
    } = useLucideContext() ?? {};
    const calculatedStrokeWidth = absoluteStrokeWidth ?? contextAbsoluteStrokeWidth ? Number(strokeWidth ?? contextStrokeWidth) * 24 / Number(size ?? contextSize) : strokeWidth ?? contextStrokeWidth;
    return createElement(
      "svg",
      {
        ref,
        ...defaultAttributes,
        width: size ?? contextSize ?? defaultAttributes.width,
        height: size ?? contextSize ?? defaultAttributes.height,
        stroke: color ?? contextColor,
        strokeWidth: calculatedStrokeWidth,
        className: mergeClasses("lucide", contextClass, className),
        ...!children && !hasA11yProp(rest) && { "aria-hidden": "true" },
        ...rest
      },
      [
        ...iconNode.map(([tag, attrs]) => createElement(tag, attrs)),
        ...Array.isArray(children) ? children : [children]
      ]
    );
  }
);

// node_modules/lucide-react/dist/esm/createLucideIcon.mjs
var createLucideIcon = (iconName, iconNode) => {
  const Component = forwardRef(
    ({ className, ...props }, ref) => createElement(Icon, {
      ref,
      iconNode,
      className: mergeClasses(
        `lucide-${toKebabCase(toPascalCase(iconName))}`,
        `lucide-${iconName}`,
        className
      ),
      ...props
    })
  );
  Component.displayName = toPascalCase(iconName);
  return Component;
};

// node_modules/lucide-react/dist/esm/icons/app-window.mjs
var __iconNode = [
  ["rect", { x: "2", y: "4", width: "20", height: "16", rx: "2", key: "izxlao" }],
  ["path", { d: "M10 4v4", key: "pp8u80" }],
  ["path", { d: "M2 8h20", key: "d11cs7" }],
  ["path", { d: "M6 4v4", key: "1svtjw" }]
];
var AppWindow = createLucideIcon("app-window", __iconNode);

// node_modules/lucide-react/dist/esm/icons/bell-dot.mjs
var __iconNode2 = [
  ["path", { d: "M10.268 21a2 2 0 0 0 3.464 0", key: "vwvbt9" }],
  [
    "path",
    {
      d: "M11.68 2.009A6 6 0 0 0 6 8c0 4.499-1.411 5.956-2.738 7.326A1 1 0 0 0 4 17h16a1 1 0 0 0 .74-1.673c-.824-.85-1.678-1.731-2.21-3.348",
      key: "xaq59h"
    }
  ],
  ["circle", { cx: "18", cy: "5", r: "3", key: "gq8acd" }]
];
var BellDot = createLucideIcon("bell-dot", __iconNode2);

// node_modules/lucide-react/dist/esm/icons/boxes.mjs
var __iconNode3 = [
  [
    "path",
    {
      d: "M2.97 12.92A2 2 0 0 0 2 14.63v3.24a2 2 0 0 0 .97 1.71l3 1.8a2 2 0 0 0 2.06 0L12 19v-5.5l-5-3-4.03 2.42Z",
      key: "lc1i9w"
    }
  ],
  ["path", { d: "m7 16.5-4.74-2.85", key: "1o9zyk" }],
  ["path", { d: "m7 16.5 5-3", key: "va8pkn" }],
  ["path", { d: "M7 16.5v5.17", key: "jnp8gn" }],
  [
    "path",
    {
      d: "M12 13.5V19l3.97 2.38a2 2 0 0 0 2.06 0l3-1.8a2 2 0 0 0 .97-1.71v-3.24a2 2 0 0 0-.97-1.71L17 10.5l-5 3Z",
      key: "8zsnat"
    }
  ],
  ["path", { d: "m17 16.5-5-3", key: "8arw3v" }],
  ["path", { d: "m17 16.5 4.74-2.85", key: "8rfmw" }],
  ["path", { d: "M17 16.5v5.17", key: "k6z78m" }],
  [
    "path",
    {
      d: "M7.97 4.42A2 2 0 0 0 7 6.13v4.37l5 3 5-3V6.13a2 2 0 0 0-.97-1.71l-3-1.8a2 2 0 0 0-2.06 0l-3 1.8Z",
      key: "1xygjf"
    }
  ],
  ["path", { d: "M12 8 7.26 5.15", key: "1vbdud" }],
  ["path", { d: "m12 8 4.74-2.85", key: "3rx089" }],
  ["path", { d: "M12 13.5V8", key: "1io7kd" }]
];
var Boxes = createLucideIcon("boxes", __iconNode3);

// node_modules/lucide-react/dist/esm/icons/check.mjs
var __iconNode4 = [["path", { d: "M20 6 9 17l-5-5", key: "1gmf2c" }]];
var Check = createLucideIcon("check", __iconNode4);

// node_modules/lucide-react/dist/esm/icons/external-link.mjs
var __iconNode5 = [
  ["path", { d: "M15 3h6v6", key: "1q9fwt" }],
  ["path", { d: "M10 14 21 3", key: "gplh6r" }],
  ["path", { d: "M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6", key: "a6xqqp" }]
];
var ExternalLink = createLucideIcon("external-link", __iconNode5);

// node_modules/lucide-react/dist/esm/icons/key-round.mjs
var __iconNode6 = [
  [
    "path",
    {
      d: "M2.586 17.414A2 2 0 0 0 2 18.828V21a1 1 0 0 0 1 1h3a1 1 0 0 0 1-1v-1a1 1 0 0 1 1-1h1a1 1 0 0 0 1-1v-1a1 1 0 0 1 1-1h.172a2 2 0 0 0 1.414-.586l.814-.814a6.5 6.5 0 1 0-4-4z",
      key: "1s6t7t"
    }
  ],
  ["circle", { cx: "16.5", cy: "7.5", r: ".5", fill: "currentColor", key: "w0ekpg" }]
];
var KeyRound = createLucideIcon("key-round", __iconNode6);

// node_modules/lucide-react/dist/esm/icons/loader-circle.mjs
var __iconNode7 = [["path", { d: "M21 12a9 9 0 1 1-6.219-8.56", key: "13zald" }]];
var LoaderCircle = createLucideIcon("loader-circle", __iconNode7);

// node_modules/lucide-react/dist/esm/icons/pen.mjs
var __iconNode8 = [
  [
    "path",
    {
      d: "M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z",
      key: "1a8usu"
    }
  ]
];
var Pen = createLucideIcon("pen", __iconNode8);

// node_modules/lucide-react/dist/esm/icons/plus.mjs
var __iconNode9 = [
  ["path", { d: "M5 12h14", key: "1ays0h" }],
  ["path", { d: "M12 5v14", key: "s699le" }]
];
var Plus = createLucideIcon("plus", __iconNode9);

// node_modules/lucide-react/dist/esm/icons/refresh-cw.mjs
var __iconNode10 = [
  ["path", { d: "M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8", key: "v9h5vc" }],
  ["path", { d: "M21 3v5h-5", key: "1q7to0" }],
  ["path", { d: "M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16", key: "3uifl3" }],
  ["path", { d: "M8 16H3v5", key: "1cv678" }]
];
var RefreshCw = createLucideIcon("refresh-cw", __iconNode10);

// node_modules/lucide-react/dist/esm/icons/rotate-ccw.mjs
var __iconNode11 = [
  ["path", { d: "M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8", key: "1357e3" }],
  ["path", { d: "M3 3v5h5", key: "1xhq8a" }]
];
var RotateCcw = createLucideIcon("rotate-ccw", __iconNode11);

// node_modules/lucide-react/dist/esm/icons/settings-2.mjs
var __iconNode12 = [
  ["path", { d: "M14 17H5", key: "gfn3mx" }],
  ["path", { d: "M19 7h-9", key: "6i9tg" }],
  ["circle", { cx: "17", cy: "17", r: "3", key: "18b49y" }],
  ["circle", { cx: "7", cy: "7", r: "3", key: "dfmy0x" }]
];
var Settings2 = createLucideIcon("settings-2", __iconNode12);

// node_modules/lucide-react/dist/esm/icons/terminal.mjs
var __iconNode13 = [
  ["path", { d: "M12 19h8", key: "baeox8" }],
  ["path", { d: "m4 17 6-6-6-6", key: "1yngyt" }]
];
var Terminal = createLucideIcon("terminal", __iconNode13);

// node_modules/lucide-react/dist/esm/icons/toggle-left.mjs
var __iconNode14 = [
  ["circle", { cx: "9", cy: "12", r: "3", key: "u3jwor" }],
  ["rect", { width: "20", height: "14", x: "2", y: "5", rx: "7", key: "g7kal2" }]
];
var ToggleLeft = createLucideIcon("toggle-left", __iconNode14);

// node_modules/lucide-react/dist/esm/icons/toggle-right.mjs
var __iconNode15 = [
  ["circle", { cx: "15", cy: "12", r: "3", key: "1afu0r" }],
  ["rect", { width: "20", height: "14", x: "2", y: "5", rx: "7", key: "g7kal2" }]
];
var ToggleRight = createLucideIcon("toggle-right", __iconNode15);

// node_modules/lucide-react/dist/esm/icons/trash-2.mjs
var __iconNode16 = [
  ["path", { d: "M10 11v6", key: "nco0om" }],
  ["path", { d: "M14 11v6", key: "outv1u" }],
  ["path", { d: "M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6", key: "miytrc" }],
  ["path", { d: "M3 6h18", key: "d0wm0j" }],
  ["path", { d: "M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2", key: "e791ji" }]
];
var Trash2 = createLucideIcon("trash-2", __iconNode16);

// node_modules/lucide-react/dist/esm/icons/unlink.mjs
var __iconNode17 = [
  [
    "path",
    {
      d: "m18.84 12.25 1.72-1.71h-.02a5.004 5.004 0 0 0-.12-7.07 5.006 5.006 0 0 0-6.95 0l-1.72 1.71",
      key: "yqzxt4"
    }
  ],
  [
    "path",
    {
      d: "m5.17 11.75-1.71 1.71a5.004 5.004 0 0 0 .12 7.07 5.006 5.006 0 0 0 6.95 0l1.71-1.71",
      key: "4qinb0"
    }
  ],
  ["line", { x1: "8", x2: "8", y1: "2", y2: "5", key: "1041cp" }],
  ["line", { x1: "2", x2: "5", y1: "8", y2: "8", key: "14m1p5" }],
  ["line", { x1: "16", x2: "16", y1: "19", y2: "22", key: "rzdirn" }],
  ["line", { x1: "19", x2: "22", y1: "16", y2: "16", key: "ox905f" }]
];
var Unlink = createLucideIcon("unlink", __iconNode17);

// node_modules/lucide-react/dist/esm/icons/users.mjs
var __iconNode18 = [
  ["path", { d: "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2", key: "1yyitq" }],
  ["path", { d: "M16 3.128a4 4 0 0 1 0 7.744", key: "16gr8j" }],
  ["path", { d: "M22 21v-2a4 4 0 0 0-3-3.87", key: "kshegd" }],
  ["circle", { cx: "9", cy: "7", r: "4", key: "nufk8" }]
];
var Users = createLucideIcon("users", __iconNode18);

// node_modules/lucide-react/dist/esm/icons/wrench.mjs
var __iconNode19 = [
  [
    "path",
    {
      d: "M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.106-3.105c.32-.322.863-.22.983.218a6 6 0 0 1-8.259 7.057l-7.91 7.91a1 1 0 0 1-2.999-3l7.91-7.91a6 6 0 0 1 7.057-8.259c.438.12.54.662.219.984z",
      key: "1ngwbx"
    }
  ]
];
var Wrench = createLucideIcon("wrench", __iconNode19);

// node_modules/lucide-react/dist/esm/icons/x.mjs
var __iconNode20 = [
  ["path", { d: "M18 6 6 18", key: "1bl5f8" }],
  ["path", { d: "m6 6 12 12", key: "d8bk6v" }]
];
var X = createLucideIcon("x", __iconNode20);

// src/shared/components/ui.tsx
function CardInner({ children }) {
  return /* @__PURE__ */ jsx("div", { className: "sarmg-card__inner", children });
}
function CardRow({
  label,
  children,
  span,
  row,
  chart
}) {
  const gridRow = row ? String(row) : span ? `span ${span}` : void 0;
  return /* @__PURE__ */ jsxs(
    "div",
    {
      className: `sarmg-card__row${chart ? " sarmg-card__row-chart" : ""}`,
      style: gridRow ? { gridRow } : void 0,
      children: [
        /* @__PURE__ */ jsx("span", { className: "sarmg-card__label", children: label }),
        /* @__PURE__ */ jsx("div", { className: "sarmg-card__content", children })
      ]
    }
  );
}
function CardActions({
  children,
  label = "\u64CD\u4F5C",
  className = "",
  onClick
}) {
  return /* @__PURE__ */ jsx(CardRow, { label, row: 6, children: /* @__PURE__ */ jsx("div", { className: `sarmg-card__actions${className ? ` ${className}` : ""}`, onClick, children }) });
}
function ActionButton({
  icon: Icon2,
  label,
  busy,
  disabled,
  tone = "primary",
  onClick
}) {
  return /* @__PURE__ */ jsxs(
    "button",
    {
      className: `action-button ${tone}`,
      type: "button",
      onClick,
      disabled: busy || disabled,
      title: label,
      children: [
        busy ? /* @__PURE__ */ jsx(LoaderCircle, { className: "spin", size: 16 }) : /* @__PURE__ */ jsx(Icon2, { size: 16 }),
        /* @__PURE__ */ jsx("span", { children: label })
      ]
    }
  );
}
function SectionHeader({
  icon: Icon2,
  title,
  description,
  actions
}) {
  return /* @__PURE__ */ jsxs("div", { className: "section-header", children: [
    /* @__PURE__ */ jsx(ContentTitle, { icon: Icon2, title, description }),
    actions ? /* @__PURE__ */ jsx("div", { className: "section-actions", children: actions }) : null
  ] });
}
function ContentTitle({ icon: Icon2, title, description }) {
  return /* @__PURE__ */ jsxs("div", { className: "section-title", children: [
    /* @__PURE__ */ jsx(Icon2, { size: 18 }),
    /* @__PURE__ */ jsxs("div", { children: [
      /* @__PURE__ */ jsx("h2", { children: title }),
      description ? /* @__PURE__ */ jsx("p", { children: description }) : null
    ] })
  ] });
}
function StatusLed({ tone }) {
  return /* @__PURE__ */ jsx("span", { className: `sarmg-status-led sarmg-status-${tone}`, "aria-hidden": "true" });
}
function InlineNotice({
  tone,
  text
}) {
  return /* @__PURE__ */ jsxs("div", { className: `inline-notice ${tone}`, role: tone === "danger" ? "alert" : "status", "aria-live": tone === "danger" ? "assertive" : "polite", children: [
    /* @__PURE__ */ jsx(BellDot, { size: 16, "aria-hidden": "true" }),
    /* @__PURE__ */ jsx("span", { children: text })
  ] });
}
function MutationError({
  mutation
}) {
  if (!mutation.isError || !mutation.error) {
    return null;
  }
  return /* @__PURE__ */ jsx(InlineNotice, { tone: "danger", text: mutation.error.message });
}
function LoadingBlock({ label }) {
  return /* @__PURE__ */ jsxs("div", { className: "loading-block", role: "status", "aria-live": "polite", children: [
    /* @__PURE__ */ jsx(LoaderCircle, { className: "spin", size: 18, "aria-hidden": "true" }),
    /* @__PURE__ */ jsx("span", { children: label })
  ] });
}

// src/shared/lib/adjacentPanel.ts
function adjacentPanelLayout({
  cardWidth,
  cardHeight,
  columnGap,
  rowGap,
  column,
  columnCount,
  top
}) {
  const panelColumns = Math.min(3, columnCount);
  const opensRight = column < Math.ceil(columnCount / 2);
  const requestedStart = opensRight ? column + 1 : column - panelColumns;
  const startColumn = Math.max(0, Math.min(requestedStart, columnCount - panelColumns));
  return {
    left: startColumn * (cardWidth + columnGap),
    top,
    width: panelColumns * cardWidth + (panelColumns - 1) * columnGap,
    height: 3 * cardHeight + 2 * rowGap,
    placement: opensRight ? "right" : "left"
  };
}

// src/features/sunshine/queryKeys.ts
var sunshineQueryKeys = {
  sunshine: {
    hosts: ["sunshine-hosts"],
    apps: (hostId) => ["sunshine-apps", hostId],
    clients: (hostId) => ["sunshine-clients", hostId],
    config: (hostId) => ["sunshine-config", hostId]
  },
  logs: { sunshine: (hostId) => ["logs", "sunshine", hostId] }
};

// src/features/sunshine/queries.ts
function updateVariables(value) {
  if (!value || typeof value !== "object") return null;
  const candidate = value;
  if (typeof candidate.id !== "string" || !candidate.patch || typeof candidate.patch !== "object") return null;
  return { id: candidate.id, patch: candidate.patch };
}
function savedHost(value, id) {
  if (!value || typeof value !== "object") return void 0;
  const candidate = value;
  return candidate.id === id ? value : void 0;
}
async function querySunshineHosts(queryClient, signal) {
  const mutationCache = queryClient.getMutationCache();
  const createMutationsAtStart = new Map(
    mutationCache.findAll({
      mutationKey: sunshineHostMutationKeys.create,
      exact: true
    }).map((mutation) => [mutation, mutation.state.status])
  );
  const updateMutationsAtStart = new Map(
    mutationCache.findAll({
      mutationKey: sunshineHostMutationKeys.update,
      exact: true
    }).map((mutation) => [mutation, mutation.state.status])
  );
  const deleteMutationsAtStart = new Map(
    mutationCache.findAll({
      mutationKey: sunshineHostMutationKeys.delete,
      exact: true
    }).map((mutation) => [mutation, mutation.state.status])
  );
  const remote = await sunshineApi.sunshineHosts(signal);
  const current = queryClient.getQueryData(
    sunshineQueryKeys.sunshine.hosts
  ) ?? [];
  const deletingIds = new Set(mutationCache.findAll({
    mutationKey: sunshineHostMutationKeys.delete,
    exact: true
  }).flatMap((mutation) => {
    const id = mutation.state.variables;
    if (typeof id !== "string" || mutation.state.status === "error") return [];
    return deleteMutationsAtStart.get(mutation) === "success" ? [] : [id];
  }));
  const updateOverlays = mutationCache.findAll({
    mutationKey: sunshineHostMutationKeys.update,
    exact: true
  }).flatMap((mutation) => {
    const variables = updateVariables(mutation.state.variables);
    if (!variables || mutation.state.status === "error") return [];
    if (updateMutationsAtStart.get(mutation) === "success") return [];
    return [{
      ...variables,
      saved: mutation.state.status === "success" ? savedHost(mutation.state.data, variables.id) : void 0
    }];
  });
  const createdHosts = mutationCache.findAll({
    mutationKey: sunshineHostMutationKeys.create,
    exact: true
  }).flatMap((mutation) => {
    if (mutation.state.status !== "success" || createMutationsAtStart.get(mutation) === "success") return [];
    const result = mutation.state.data;
    if (!result || typeof result !== "object" || typeof result.id !== "string") return [];
    return [result];
  });
  return mergeSunshineHostSnapshot(
    remote,
    current,
    deletingIds,
    updateOverlays,
    createdHosts
  );
}

// src/shared/components/InlineEditableField.tsx
function InlineEditableField({
  value,
  label,
  validate,
  onSave,
  compact = false,
  displayValue,
  inputType = "text",
  normalize = (next) => next.trim(),
  cancelEmpty = false,
  maxLength,
  disabled = false
}) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(value);
  const [error, setError] = useState("");
  const errorId = useId();
  const committingRef = useRef(false);
  const skipBlurRef = useRef(false);
  const cancel = () => {
    skipBlurRef.current = true;
    setDraft(value);
    setError("");
    setEditing(false);
  };
  const commit = async () => {
    if (committingRef.current) return;
    const next = normalize(draft);
    if (cancelEmpty && next.length === 0) {
      setDraft(value);
      setError("");
      setEditing(false);
      return;
    }
    const validationError = validate(next);
    if (validationError) {
      setError(validationError);
      return;
    }
    if (next === value) {
      setEditing(false);
      return;
    }
    committingRef.current = true;
    try {
      await onSave(next);
      setDraft(inputType === "password" ? value : next);
      setError("");
      setEditing(false);
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : "\u4FDD\u5B58\u5931\u8D25");
    } finally {
      committingRef.current = false;
    }
  };
  if (editing) {
    return /* @__PURE__ */ jsxs(Fragment, { children: [
      /* @__PURE__ */ jsx(
        "input",
        {
          className: `sunshine-inline-input${compact ? " compact" : ""}${error ? " input-error" : ""}`,
          value: draft,
          type: inputType,
          "aria-label": label,
          "aria-invalid": Boolean(error),
          "aria-errormessage": error ? errorId : void 0,
          title: error || void 0,
          maxLength,
          autoFocus: true,
          onClick: (event) => event.stopPropagation(),
          onChange: (event) => {
            setDraft(event.target.value);
            setError("");
          },
          onBlur: () => {
            if (skipBlurRef.current) {
              skipBlurRef.current = false;
              return;
            }
            void commit();
          },
          onKeyDown: (event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              void commit();
            }
            if (event.key === "Escape") {
              event.preventDefault();
              cancel();
            }
          }
        }
      ),
      error ? /* @__PURE__ */ jsx("span", { className: "sr-only", id: errorId, role: "alert", children: error }) : null
    ] });
  }
  return /* @__PURE__ */ jsx(
    "button",
    {
      type: "button",
      className: `sunshine-inline-editable${compact ? " compact" : ""}`,
      title: disabled ? "\u6B63\u5728\u4FDD\u5B58\uFF0C\u8BF7\u7A0D\u5019" : `\u4FEE\u6539${label}`,
      "aria-label": `\u4FEE\u6539${label}\uFF0C\u5F53\u524D\u503C\uFF1A${displayValue ?? value}`,
      disabled,
      onClick: (event) => {
        event.stopPropagation();
        if (disabled) return;
        skipBlurRef.current = false;
        setDraft(value);
        setEditing(true);
      },
      children: displayValue ?? value
    }
  );
}

// src/features/sunshine/components/HostCard.tsx
var RE_IPV4 = /^((25[0-5]|2[0-4]\d|1\d{2}|[1-9]\d|\d)\.){3}(25[0-5]|2[0-4]\d|1\d{2}|[1-9]\d|\d)$/;
var RE_DOMAIN = /^(?!-)[A-Za-z0-9-]{1,63}(?<!-)(\.[A-Za-z0-9-]{1,63}(?<!-))*\.?$/;
function isValidIpv6(value) {
  const inner = value.startsWith("[") && value.endsWith("]") ? value.slice(1, -1) : value;
  if (!inner.includes(":")) return false;
  try {
    return new URL(`http://[${inner}]/`).hostname.startsWith("[");
  } catch {
    return false;
  }
}
function isValidHost(value) {
  return RE_IPV4.test(value) || isValidIpv6(value) || RE_DOMAIN.test(value);
}
function HostCard({ host, selected, updating, canWrite, canManage, onOpen, onDelete, onInlineUpdate }) {
  const probePending = host.probe_status === "pending";
  const optimistic = isOptimisticSunshineHost(host);
  const controlsDisabled = optimistic || updating;
  const editingDisabled = controlsDisabled || !canWrite;
  const managementDisabled = controlsDisabled || !canManage;
  const connectionLabel = probePending ? host.connection_error ?? "\u6B63\u5728\u68C0\u6D4B Sunshine \u8FDE\u63A5" : host.connected ? "Sunshine API \u5DF2\u8FDE\u63A5" : host.connection_error ?? "Sunshine API \u672A\u8FDE\u63A5";
  const submitQuickPatch = (patch) => {
    void onInlineUpdate(patch).catch(() => void 0);
  };
  return /* @__PURE__ */ jsx(
    "article",
    {
      className: `sarmg-card service-card sunshine-host-card${selected ? " active" : ""}`,
      "aria-busy": controlsDisabled,
      "aria-label": `${host.name}\uFF0C${connectionLabel}`,
      children: /* @__PURE__ */ jsxs(CardInner, { children: [
        /* @__PURE__ */ jsxs(CardRow, { label: "\u540D\u79F0", children: [
          /* @__PURE__ */ jsx(
            InlineEditableField,
            {
              label: "\u540D\u79F0",
              value: host.name,
              validate: (value) => value && value.length <= 128 ? null : "\u540D\u79F0\u5FC5\u987B\u4E3A 1\u2013128 \u4E2A\u5B57\u7B26",
              onSave: (name) => onInlineUpdate({ name }),
              maxLength: 128,
              disabled: editingDisabled
            }
          ),
          /* @__PURE__ */ jsxs("span", { title: connectionLabel, children: [
            /* @__PURE__ */ jsx(StatusLed, { tone: probePending ? "warn" : host.connected ? "good" : "danger" }),
            /* @__PURE__ */ jsx("span", { className: "sr-only", children: connectionLabel })
          ] })
        ] }),
        /* @__PURE__ */ jsx(CardRow, { label: "\u5730\u5740", children: /* @__PURE__ */ jsxs("div", { className: "card-address-inline", children: [
          /* @__PURE__ */ jsx(
            InlineEditableField,
            {
              label: "\u5730\u5740",
              value: host.host,
              validate: (value) => isValidHost(value) ? null : "\u8BF7\u8F93\u5165\u6709\u6548\u7684 IPv4\u3001IPv6 \u6216\u57DF\u540D",
              onSave: (address) => onInlineUpdate({ host: address }),
              maxLength: 253,
              disabled: editingDisabled
            }
          ),
          /* @__PURE__ */ jsx("span", { className: "sunshine-inline-separator", children: ":" }),
          /* @__PURE__ */ jsx(
            InlineEditableField,
            {
              label: "\u7AEF\u53E3",
              value: String(host.web_port),
              compact: true,
              validate: (value) => {
                const port = Number(value);
                return Number.isInteger(port) && port >= 1 && port <= 65535 ? null : "\u7AEF\u53E3\u5FC5\u987B\u662F 1\u201365535 \u7684\u6574\u6570";
              },
              onSave: (port) => onInlineUpdate({ web_port: Number(port) }),
              disabled: editingDisabled
            }
          )
        ] }) }),
        /* @__PURE__ */ jsx(CardRow, { label: "\u8D26\u53F7", children: /* @__PURE__ */ jsx(
          InlineEditableField,
          {
            label: "\u8D26\u53F7",
            value: host.username,
            validate: (value) => value && value.length <= 256 ? null : "\u8D26\u53F7\u5FC5\u987B\u4E3A 1\u2013256 \u4E2A\u5B57\u7B26",
            onSave: (username) => onInlineUpdate({ username }),
            maxLength: 256,
            disabled: editingDisabled
          }
        ) }),
        /* @__PURE__ */ jsxs(CardRow, { label: "\u5BC6\u7801", children: [
          /* @__PURE__ */ jsx(
            InlineEditableField,
            {
              label: "\u5BC6\u7801",
              value: "",
              displayValue: host.password_set ? "\u5DF2\u8BBE\u7F6E" : "\u672A\u8BBE\u7F6E",
              inputType: "password",
              validate: (value) => value.length <= 4096 ? null : "\u5BC6\u7801\u4E0D\u80FD\u8D85\u8FC7 4096 \u4E2A\u5B57\u7B26",
              onSave: (password) => onInlineUpdate({ password }),
              normalize: (value) => value,
              cancelEmpty: true,
              maxLength: 4096,
              disabled: editingDisabled
            }
          ),
          host.password_set ? /* @__PURE__ */ jsx(
            "button",
            {
              type: "button",
              className: "sarmg-card__action sarmg-action-danger",
              disabled: editingDisabled,
              "aria-label": `\u6E05\u7A7A ${host.name} \u7684 Sunshine \u5BC6\u7801`,
              title: "\u6E05\u7A7A\u5BC6\u7801",
              onClick: () => {
                if (window.confirm("\u786E\u5B9A\u6E05\u7A7A\u8BE5 Sunshine \u4E3B\u673A\u7684\u5BC6\u7801\uFF1F")) {
                  submitQuickPatch({ password: "" });
                }
              },
              children: "\u6E05\u7A7A"
            }
          ) : null
        ] }),
        /* @__PURE__ */ jsx(CardRow, { label: "TLS", children: /* @__PURE__ */ jsx(
          "button",
          {
            type: "button",
            className: "sarmg-card__action",
            disabled: editingDisabled,
            title: controlsDisabled ? "\u6B63\u5728\u4FDD\u5B58\u4E3B\u673A\uFF0C\u8BF7\u7A0D\u5019" : "\u4EC5\u5F00\u53D1\u6A21\u5F0F\u5141\u8BB8\u5173\u95ED\u8BC1\u4E66\u9A8C\u8BC1\uFF1B\u751F\u4EA7\u6A21\u5F0F\u4F1A\u62D2\u7EDD\u6B64\u64CD\u4F5C",
            onClick: () => {
              if (!host.verify_tls || window.confirm("\u4EC5\u5F00\u53D1\u6A21\u5F0F\u5141\u8BB8\u5173\u95ED TLS \u8BC1\u4E66\u9A8C\u8BC1\uFF1B\u751F\u4EA7\u6A21\u5F0F\u4F1A\u62D2\u7EDD\u3002\u4ECD\u8981\u5C1D\u8BD5\u5417\uFF1F")) {
                submitQuickPatch({ verify_tls: !host.verify_tls });
              }
            },
            children: host.verify_tls ? "\u9A8C\u8BC1\u8BC1\u4E66" : "\u5141\u8BB8\u81EA\u7B7E\u540D"
          }
        ) }),
        /* @__PURE__ */ jsxs(CardActions, { children: [
          /* @__PURE__ */ jsxs(
            "button",
            {
              type: "button",
              className: "sarmg-card__action",
              disabled: managementDisabled,
              onClick: (event) => onOpen(event.currentTarget),
              children: [
                /* @__PURE__ */ jsx(Pen, { size: 12 }),
                /* @__PURE__ */ jsx("span", { children: selected ? "\u6536\u8D77\u7BA1\u7406" : "\u7BA1\u7406" })
              ]
            }
          ),
          /* @__PURE__ */ jsxs("button", { type: "button", className: "sarmg-card__action sarmg-action-danger", disabled: editingDisabled, onClick: onDelete, children: [
            /* @__PURE__ */ jsx(Trash2, { size: 12 }),
            /* @__PURE__ */ jsx("span", { children: "\u5220\u9664" })
          ] }),
          /* @__PURE__ */ jsxs(
            "a",
            {
              href: controlsDisabled ? void 0 : host.web_url,
              target: "_blank",
              rel: "noopener noreferrer",
              className: "sarmg-card__action sarmg-action-primary",
              "aria-disabled": controlsDisabled,
              tabIndex: controlsDisabled ? -1 : void 0,
              onClick: (event) => {
                if (controlsDisabled) event.preventDefault();
              },
              children: [
                /* @__PURE__ */ jsx(ExternalLink, { size: 12 }),
                /* @__PURE__ */ jsx("span", { children: "\u6253\u5F00" })
              ]
            }
          )
        ] })
      ] })
    }
  );
}

// src/shared/lib/mutations.ts
function removeMutationFromCache(queryClient, mutationKey, variables) {
  const mutationCache = queryClient.getMutationCache();
  for (const mutation of mutationCache.findAll({ mutationKey, exact: true })) {
    if (variables !== void 0 && mutation.state.variables !== variables) continue;
    mutationCache.remove(mutation);
  }
}

// src/shared/lib/tabs.ts
var TAB_NAVIGATION_KEYS = /* @__PURE__ */ new Set(["ArrowLeft", "ArrowRight", "Home", "End"]);
function activateTabFromKeyboard(event, tabs, currentIndex, activate2) {
  if (!TAB_NAVIGATION_KEYS.has(event.key) || tabs.length === 0) return;
  event.preventDefault();
  let nextIndex = currentIndex;
  if (event.key === "Home") nextIndex = 0;
  else if (event.key === "End") nextIndex = tabs.length - 1;
  else if (event.key === "ArrowLeft") nextIndex = (currentIndex - 1 + tabs.length) % tabs.length;
  else if (event.key === "ArrowRight") nextIndex = (currentIndex + 1) % tabs.length;
  activate2(tabs[nextIndex]);
  const tabElements = event.currentTarget.closest('[role="tablist"]')?.querySelectorAll('[role="tab"]');
  tabElements?.[nextIndex]?.focus();
}

// src/features/sunshine/components/AppsSection.tsx
function appDraft(app) {
  const workingDirectory = typeof app["working-dir"] === "string" ? app["working-dir"] : "";
  const autoDetach = typeof app["auto-detach"] === "boolean" ? app["auto-detach"] : true;
  const waitAll = typeof app["wait-all"] === "boolean" ? app["wait-all"] : true;
  const exitTimeout = typeof app["exit-timeout"] === "number" ? app["exit-timeout"] : 5;
  return {
    ...app,
    name: typeof app.name === "string" ? app.name : "",
    cmd: typeof app.cmd === "string" ? app.cmd : "",
    "working-dir": workingDirectory,
    "auto-detach": autoDetach,
    "wait-all": waitAll,
    "exit-timeout": exitTimeout,
    index: app.index
  };
}
function extractApps(data) {
  if (!data) return [];
  return data.apps.map((app, index) => ({ ...app, index }));
}
function AppsSection({ host, canWrite }) {
  const queryClient = useQueryClient();
  const queryKey = sunshineQueryKeys.sunshine.apps(host.id);
  const appsQuery = useQuery({
    queryKey,
    queryFn: () => sunshineApi.sunshineApps(host.id),
    retry: false
  });
  const [draft, setDraft] = useState(null);
  const saveMutation = useMutation({
    mutationFn: (app) => sunshineApi.sunshineSaveApp(host.id, app),
    onSuccess: async () => {
      setDraft(null);
      await queryClient.invalidateQueries({ queryKey });
    }
  });
  const deleteMutation = useMutation({
    mutationFn: (index) => sunshineApi.sunshineDeleteApp(host.id, index),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey });
    }
  });
  const closeMutation = useMutation({
    mutationFn: () => sunshineApi.sunshineCloseApp(host.id),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey });
    }
  });
  const apps = extractApps(appsQuery.data);
  return /* @__PURE__ */ jsxs("section", { className: "section-band", children: [
    /* @__PURE__ */ jsx(
      SectionHeader,
      {
        icon: AppWindow,
        title: "\u5E94\u7528",
        actions: canWrite ? /* @__PURE__ */ jsxs("div", { className: "button-row", children: [
          /* @__PURE__ */ jsx(
            ActionButton,
            {
              icon: X,
              label: "\u7ED3\u675F\u4F1A\u8BDD",
              tone: "danger",
              busy: closeMutation.isPending,
              onClick: () => window.confirm("\u7ED3\u675F\u5F53\u524D\u5E94\u7528\u4F1A\u8BDD\uFF1F") && closeMutation.mutate()
            }
          ),
          /* @__PURE__ */ jsx(
            ActionButton,
            {
              icon: Plus,
              label: "\u65B0\u5EFA",
              onClick: () => setDraft({
                name: "",
                cmd: "",
                "working-dir": "",
                "auto-detach": true,
                "wait-all": true,
                "exit-timeout": 5,
                index: -1
              })
            }
          )
        ] }) : void 0
      }
    ),
    /* @__PURE__ */ jsx(MutationError, { mutation: saveMutation }),
    /* @__PURE__ */ jsx(MutationError, { mutation: deleteMutation }),
    /* @__PURE__ */ jsx(MutationError, { mutation: closeMutation }),
    draft ? /* @__PURE__ */ jsxs("div", { className: "sunshine-app-form", children: [
      /* @__PURE__ */ jsxs("div", { className: "sunshine-form-header", children: [
        /* @__PURE__ */ jsx("strong", { children: draft.index === -1 ? "\u65B0\u5EFA\u5E94\u7528" : "\u7F16\u8F91\u5E94\u7528" }),
        /* @__PURE__ */ jsx("button", { className: "icon-button", type: "button", "aria-label": "\u5173\u95ED\u5E94\u7528\u7F16\u8F91\u5668", title: "\u5173\u95ED", onClick: () => setDraft(null), children: /* @__PURE__ */ jsx(X, { size: 16, "aria-hidden": "true" }) })
      ] }),
      /* @__PURE__ */ jsxs("div", { className: "sunshine-form-grid", children: [
        /* @__PURE__ */ jsxs("label", { className: "inline-field wide", children: [
          /* @__PURE__ */ jsx("span", { children: "\u540D\u79F0 *" }),
          /* @__PURE__ */ jsx("input", { value: draft.name, onChange: (event) => setDraft((value) => value && { ...value, name: event.target.value }), autoFocus: true })
        ] }),
        /* @__PURE__ */ jsxs("label", { className: "inline-field wide", children: [
          /* @__PURE__ */ jsx("span", { children: "\u542F\u52A8\u547D\u4EE4" }),
          /* @__PURE__ */ jsx("input", { value: draft.cmd, onChange: (event) => setDraft((value) => value && { ...value, cmd: event.target.value }), placeholder: "\u7559\u7A7A=\u684C\u9762\u4E32\u6D41" })
        ] }),
        /* @__PURE__ */ jsxs("label", { className: "inline-field", children: [
          /* @__PURE__ */ jsx("span", { children: "\u5DE5\u4F5C\u76EE\u5F55" }),
          /* @__PURE__ */ jsx("input", { value: draft["working-dir"], onChange: (event) => setDraft((value) => value && { ...value, "working-dir": event.target.value }) })
        ] }),
        /* @__PURE__ */ jsxs("label", { className: "inline-field", children: [
          /* @__PURE__ */ jsx("span", { children: "\u9000\u51FA\u8D85\u65F6\uFF08\u79D2\uFF09" }),
          /* @__PURE__ */ jsx("input", { type: "number", min: 0, value: draft["exit-timeout"], onChange: (event) => setDraft((value) => value && { ...value, "exit-timeout": Number(event.target.value) }) })
        ] })
      ] }),
      /* @__PURE__ */ jsxs("div", { className: "button-row", children: [
        /* @__PURE__ */ jsx(
          ActionButton,
          {
            icon: Check,
            label: "\u4FDD\u5B58",
            busy: saveMutation.isPending,
            disabled: !draft.name.trim() || !Number.isFinite(draft["exit-timeout"]) || draft["exit-timeout"] < 0,
            onClick: () => saveMutation.mutate({ ...draft, name: draft.name.trim() })
          }
        ),
        /* @__PURE__ */ jsx(ActionButton, { icon: X, label: "\u53D6\u6D88", onClick: () => setDraft(null) })
      ] })
    ] }) : null,
    appsQuery.isLoading ? /* @__PURE__ */ jsx(LoadingBlock, { label: "\u8BFB\u53D6\u5E94\u7528" }) : null,
    appsQuery.error ? /* @__PURE__ */ jsx(InlineNotice, { tone: "danger", text: appsQuery.error.message }) : null,
    /* @__PURE__ */ jsx("div", { className: "sunshine-app-list", children: apps.map((app) => /* @__PURE__ */ jsxs("div", { className: "sunshine-app-item", children: [
      /* @__PURE__ */ jsxs("div", { className: "sunshine-app-info", children: [
        /* @__PURE__ */ jsx("strong", { children: app.name }),
        /* @__PURE__ */ jsx("span", { className: "mono", children: app.cmd || "\uFF08\u684C\u9762\u4E32\u6D41\uFF09" }),
        /* @__PURE__ */ jsxs("em", { children: [
          "index: ",
          app.index
        ] })
      ] }),
      canWrite ? /* @__PURE__ */ jsxs("div", { className: "button-row", children: [
        /* @__PURE__ */ jsx("button", { className: "icon-button", type: "button", title: "\u7F16\u8F91", "aria-label": `\u7F16\u8F91\u5E94\u7528 ${app.name}`, onClick: () => setDraft(appDraft(app)), children: /* @__PURE__ */ jsx(Pen, { size: 15, "aria-hidden": "true" }) }),
        /* @__PURE__ */ jsx(
          "button",
          {
            className: "icon-button danger",
            type: "button",
            title: "\u5220\u9664",
            disabled: deleteMutation.isPending,
            "aria-label": `\u5220\u9664\u5E94\u7528 ${app.name}`,
            onClick: () => window.confirm(`\u5220\u9664\u5E94\u7528 "${app.name}"\uFF1F`) && deleteMutation.mutate(app.index),
            children: /* @__PURE__ */ jsx(Trash2, { size: 15 })
          }
        )
      ] }) : null
    ] }, String(app.index))) })
  ] });
}

// src/features/sunshine/components/ClientsSection.tsx
function extractClients(data) {
  return data ? data.named_certs : [];
}
function ClientsSection({ host, canWrite }) {
  const queryClient = useQueryClient();
  const queryKey = sunshineQueryKeys.sunshine.clients(host.id);
  const query = useQuery({ queryKey, queryFn: () => sunshineApi.sunshineClients(host.id) });
  const unpairMutation = useMutation({
    mutationFn: (uuid) => sunshineApi.sunshineUnpairClient(host.id, uuid),
    onSuccess: async () => queryClient.invalidateQueries({ queryKey })
  });
  const unpairAllMutation = useMutation({
    mutationFn: () => sunshineApi.sunshineUnpairAll(host.id),
    onSuccess: async () => queryClient.invalidateQueries({ queryKey })
  });
  const updateMutation = useMutation({
    mutationFn: ({ uuid, enabled }) => sunshineApi.sunshineUpdateClient(host.id, uuid, enabled),
    onSuccess: async () => queryClient.invalidateQueries({ queryKey })
  });
  const clients = extractClients(query.data);
  return /* @__PURE__ */ jsxs("section", { className: "section-band", children: [
    /* @__PURE__ */ jsx(
      SectionHeader,
      {
        icon: Users,
        title: "\u5BA2\u6237\u7AEF",
        actions: canWrite ? /* @__PURE__ */ jsx(
          ActionButton,
          {
            icon: Unlink,
            label: "\u53D6\u6D88\u6240\u6709\u914D\u5BF9",
            tone: "danger",
            busy: unpairAllMutation.isPending,
            onClick: () => window.confirm("\u53D6\u6D88\u6240\u6709\u914D\u5BF9\uFF1F") && unpairAllMutation.mutate()
          }
        ) : void 0
      }
    ),
    /* @__PURE__ */ jsx(MutationError, { mutation: unpairMutation }),
    /* @__PURE__ */ jsx(MutationError, { mutation: unpairAllMutation }),
    /* @__PURE__ */ jsx(MutationError, { mutation: updateMutation }),
    query.isLoading ? /* @__PURE__ */ jsx(LoadingBlock, { label: "\u8BFB\u53D6\u5BA2\u6237\u7AEF" }) : null,
    query.error ? /* @__PURE__ */ jsx(InlineNotice, { tone: "danger", text: query.error.message }) : null,
    /* @__PURE__ */ jsxs("div", { className: "sunshine-client-list", children: [
      clients.map((client) => /* @__PURE__ */ jsxs("div", { className: "sunshine-client-item", children: [
        /* @__PURE__ */ jsxs("div", { className: "sunshine-client-info", children: [
          /* @__PURE__ */ jsx("strong", { children: client.name ?? "\u672A\u547D\u540D\u8BBE\u5907" }),
          /* @__PURE__ */ jsx("span", { className: "mono", children: client.uuid }),
          /* @__PURE__ */ jsxs("span", { className: "sunshine-client-status", children: [
            /* @__PURE__ */ jsx(StatusLed, { tone: client.enabled ? "good" : "warn" }),
            client.enabled ? "\u5DF2\u542F\u7528" : "\u5DF2\u7981\u7528"
          ] })
        ] }),
        canWrite ? /* @__PURE__ */ jsxs("div", { className: "button-row", children: [
          /* @__PURE__ */ jsx(
            "button",
            {
              className: "icon-button",
              type: "button",
              title: client.enabled ? "\u7981\u7528" : "\u542F\u7528",
              "aria-label": `${client.enabled ? "\u7981\u7528" : "\u542F\u7528"}\u5BA2\u6237\u7AEF ${client.name ?? client.uuid}`,
              disabled: updateMutation.isPending,
              onClick: () => updateMutation.mutate({ uuid: client.uuid, enabled: !client.enabled }),
              children: client.enabled ? /* @__PURE__ */ jsx(ToggleRight, { size: 18 }) : /* @__PURE__ */ jsx(ToggleLeft, { size: 18 })
            }
          ),
          /* @__PURE__ */ jsx(
            "button",
            {
              className: "icon-button danger",
              type: "button",
              title: "\u53D6\u6D88\u914D\u5BF9",
              disabled: unpairMutation.isPending,
              "aria-label": `\u53D6\u6D88\u5BA2\u6237\u7AEF ${client.name ?? client.uuid} \u7684\u914D\u5BF9`,
              onClick: () => window.confirm(`\u53D6\u6D88\u8BBE\u5907 "${client.name ?? client.uuid}" \u7684\u914D\u5BF9\uFF1F`) && unpairMutation.mutate(client.uuid),
              children: /* @__PURE__ */ jsx(Unlink, { size: 15 })
            }
          )
        ] }) : null
      ] }, client.uuid)),
      !query.isLoading && !clients.length ? /* @__PURE__ */ jsx("p", { className: "muted-inline", children: "\u6682\u65E0\u5DF2\u914D\u5BF9\u5BA2\u6237\u7AEF\u3002" }) : null
    ] })
  ] });
}

// src/features/sunshine/components/HostPanel.tsx
var HOST_SECTIONS = [
  { key: "apps", label: "\u5E94\u7528", Icon: AppWindow },
  { key: "clients", label: "\u5BA2\u6237\u7AEF", Icon: Users },
  { key: "pairing", label: "\u914D\u5BF9", Icon: KeyRound },
  { key: "config", label: "\u914D\u7F6E", Icon: Settings2 },
  { key: "system", label: "\u7CFB\u7EDF", Icon: Wrench }
];
function pairMutationKey(hostId) {
  return ["sunshine-pair", hostId];
}
function PairingSection({ host, canWrite }) {
  const queryClient = useQueryClient();
  const [pin, setPin] = useState("");
  const [deviceName, setDeviceName] = useState("");
  const pairMutation = useMutation({
    mutationKey: pairMutationKey(host.id),
    mutationFn: ({ pin: submittedPin, deviceName: submittedDeviceName }) => sunshineApi.sunshinePin(host.id, submittedPin, submittedDeviceName),
    onSuccess: () => {
      setPin("");
      setDeviceName("");
    },
    onSettled: (_result, _error, variables) => {
      variables.pin = "";
      removeMutationFromCache(queryClient, pairMutationKey(host.id), variables);
    }
  });
  const canPair = canWrite && /^\d{4,8}$/.test(pin.trim()) && !pairMutation.isPending;
  const submitPairing = () => pairMutation.mutate({
    pin: pin.trim(),
    deviceName: deviceName.trim() || "Moonlight Client"
  });
  return /* @__PURE__ */ jsxs("section", { className: "section-band", children: [
    /* @__PURE__ */ jsx(SectionHeader, { icon: KeyRound, title: "PIN \u914D\u5BF9" }),
    /* @__PURE__ */ jsx(MutationError, { mutation: pairMutation }),
    pairMutation.isSuccess ? /* @__PURE__ */ jsx(InlineNotice, { tone: "warn", text: "\u914D\u5BF9\u8BF7\u6C42\u5DF2\u63D0\u4EA4\u3002" }) : null,
    /* @__PURE__ */ jsxs("div", { className: "sunshine-pin-form", children: [
      /* @__PURE__ */ jsxs("label", { className: "inline-field", children: [
        /* @__PURE__ */ jsx("span", { children: "PIN \u7801 *" }),
        /* @__PURE__ */ jsx(
          "input",
          {
            value: pin,
            onChange: (event) => {
              setPin(event.target.value);
              if (pairMutation.isSuccess || pairMutation.isError) pairMutation.reset();
            },
            maxLength: 8,
            disabled: !canWrite,
            minLength: 4,
            inputMode: "numeric",
            pattern: "[0-9]{4,8}",
            placeholder: "1234",
            autoFocus: true,
            onKeyDown: (event) => {
              if (event.key === "Enter" && canPair) {
                event.preventDefault();
                submitPairing();
              }
            }
          }
        )
      ] }),
      /* @__PURE__ */ jsxs("label", { className: "inline-field", children: [
        /* @__PURE__ */ jsx("span", { children: "\u8BBE\u5907\u540D\u79F0" }),
        /* @__PURE__ */ jsx("input", { value: deviceName, maxLength: 80, disabled: !canWrite, onChange: (event) => setDeviceName(event.target.value), placeholder: "Moonlight Client" })
      ] }),
      /* @__PURE__ */ jsx("div", { style: { display: "flex" }, children: /* @__PURE__ */ jsx(ActionButton, { icon: Check, label: "\u63D0\u4EA4\u914D\u5BF9", busy: pairMutation.isPending, disabled: !canPair, onClick: submitPairing }) })
    ] })
  ] });
}
function ConfigSection({ host, canWrite }) {
  const queryClient = useQueryClient();
  const queryKey = sunshineQueryKeys.sunshine.config(host.id);
  const query = useQuery({ queryKey, queryFn: () => sunshineApi.sunshineConfig(host.id) });
  const [draft, setDraft] = useState(null);
  const editMode = draft !== null;
  let parsedDraft = null;
  let draftError = "";
  if (draft !== null) {
    try {
      parsedDraft = parseSunshineConfigDraft(draft);
    } catch (error) {
      draftError = error instanceof Error ? error.message : "\u914D\u7F6E\u4E0D\u662F\u6709\u6548\u7684 JSON \u5BF9\u8C61";
    }
  }
  const saveMutation = useMutation({
    mutationFn: () => sunshineApi.sunshineSaveConfig(host.id, parseSunshineConfigDraft(draft ?? "{}")),
    onSuccess: async () => {
      setDraft(null);
      await queryClient.invalidateQueries({ queryKey });
    }
  });
  const entries = Object.entries(query.data ?? {});
  const cancelEdit = () => {
    setDraft(null);
    saveMutation.reset();
  };
  return /* @__PURE__ */ jsxs("section", { className: "section-band", children: [
    /* @__PURE__ */ jsx(
      SectionHeader,
      {
        icon: Settings2,
        title: "\u914D\u7F6E",
        actions: editMode ? /* @__PURE__ */ jsxs("div", { className: "button-row", children: [
          /* @__PURE__ */ jsx(ActionButton, { icon: Check, label: "\u4FDD\u5B58", busy: saveMutation.isPending, disabled: !parsedDraft, onClick: () => saveMutation.mutate() }),
          /* @__PURE__ */ jsx(ActionButton, { icon: X, label: "\u53D6\u6D88", onClick: cancelEdit })
        ] }) : /* @__PURE__ */ jsx(ActionButton, { icon: Pen, label: "\u7F16\u8F91 JSON", disabled: !canWrite || !query.data, onClick: () => setDraft(JSON.stringify(query.data ?? {}, null, 2)) })
      }
    ),
    query.isLoading ? /* @__PURE__ */ jsx(LoadingBlock, { label: "\u8BFB\u53D6\u914D\u7F6E" }) : null,
    query.error ? /* @__PURE__ */ jsx(InlineNotice, { tone: "danger", text: query.error.message }) : null,
    /* @__PURE__ */ jsx(MutationError, { mutation: saveMutation }),
    !editMode ? /* @__PURE__ */ jsx("div", { className: "sunshine-config-table", "aria-label": "Sunshine \u914D\u7F6E\u53EA\u8BFB\u9884\u89C8", children: entries.map(([key, value]) => /* @__PURE__ */ jsxs("div", { className: "sunshine-config-row", children: [
      /* @__PURE__ */ jsx("span", { className: "mono", children: key }),
      /* @__PURE__ */ jsx("span", { className: "mono sunshine-config-value", children: typeof value === "string" ? value : JSON.stringify(value) })
    ] }, key)) }) : /* @__PURE__ */ jsxs("div", { className: "sunshine-config-edit", children: [
      /* @__PURE__ */ jsxs("label", { className: "inline-field wide", children: [
        /* @__PURE__ */ jsx("span", { children: "\u5B8C\u6574 JSON \u914D\u7F6E\uFF08\u4FDD\u7559\u5B57\u7B26\u4E32\u3001\u6570\u5B57\u3001\u5E03\u5C14\u503C\u548C\u5BF9\u8C61\u7C7B\u578B\uFF09" }),
        /* @__PURE__ */ jsx(
          "textarea",
          {
            className: "sunshine-config-json",
            value: draft ?? "",
            onChange: (event) => setDraft(event.target.value),
            rows: 20,
            spellCheck: false,
            "aria-invalid": Boolean(draftError)
          }
        )
      ] }),
      draftError ? /* @__PURE__ */ jsx(InlineNotice, { tone: "danger", text: draftError }) : null
    ] })
  ] });
}
function SystemSection({ host, canWrite }) {
  const restartMutation = useMutation({ mutationFn: () => sunshineApi.sunshineRestart(host.id) });
  const resetMutation = useMutation({ mutationFn: () => sunshineApi.sunshineResetDisplay(host.id) });
  return /* @__PURE__ */ jsx("section", { className: "view-stack", children: /* @__PURE__ */ jsxs("section", { className: "section-band", children: [
    /* @__PURE__ */ jsx(SectionHeader, { icon: Wrench, title: "\u7CFB\u7EDF\u64CD\u4F5C" }),
    /* @__PURE__ */ jsx(MutationError, { mutation: restartMutation }),
    /* @__PURE__ */ jsx(MutationError, { mutation: resetMutation }),
    restartMutation.isSuccess ? /* @__PURE__ */ jsx(InlineNotice, { tone: "warn", text: "\u91CD\u542F\u547D\u4EE4\u5DF2\u53D1\u9001\u3002" }) : null,
    resetMutation.isSuccess ? /* @__PURE__ */ jsx(InlineNotice, { tone: "warn", text: "\u663E\u793A\u8BBE\u5907\u914D\u7F6E\u5DF2\u91CD\u7F6E\u3002" }) : null,
    /* @__PURE__ */ jsxs("div", { className: "sunshine-system-actions", children: [
      /* @__PURE__ */ jsxs("div", { className: "sunshine-system-card", children: [
        /* @__PURE__ */ jsx(RefreshCw, { size: 24 }),
        /* @__PURE__ */ jsxs("div", { children: [
          /* @__PURE__ */ jsx("strong", { children: "\u91CD\u542F Sunshine" }),
          /* @__PURE__ */ jsx("p", { children: "\u91CD\u65B0\u52A0\u8F7D\u914D\u7F6E\uFF0C\u5F53\u524D\u4E32\u6D41\u4F1A\u8BDD\u5C06\u4E2D\u65AD\u3002" })
        ] }),
        /* @__PURE__ */ jsx(
          ActionButton,
          {
            icon: RefreshCw,
            label: "\u7ACB\u5373\u91CD\u542F",
            tone: "danger",
            busy: restartMutation.isPending,
            disabled: !canWrite,
            onClick: () => window.confirm("\u786E\u5B9A\u91CD\u542F Sunshine\uFF1F\u5F53\u524D\u4F1A\u8BDD\u5C06\u4E2D\u65AD\u3002") && restartMutation.mutate()
          }
        )
      ] }),
      /* @__PURE__ */ jsxs("div", { className: "sunshine-system-card", children: [
        /* @__PURE__ */ jsx(RotateCcw, { size: 24 }),
        /* @__PURE__ */ jsxs("div", { children: [
          /* @__PURE__ */ jsx("strong", { children: "\u91CD\u7F6E\u663E\u793A\u8BBE\u5907" }),
          /* @__PURE__ */ jsx("p", { children: "\u6E05\u9664 Sunshine \u4FDD\u5B58\u7684\u663E\u793A\u8BBE\u5907\u6301\u4E45\u5316\u914D\u7F6E\u3002" })
        ] }),
        /* @__PURE__ */ jsx(
          ActionButton,
          {
            icon: RotateCcw,
            label: "\u91CD\u7F6E\u663E\u793A",
            busy: resetMutation.isPending,
            disabled: !canWrite,
            onClick: () => window.confirm("\u786E\u5B9A\u91CD\u7F6E\u663E\u793A\u8BBE\u5907\u914D\u7F6E\uFF1F") && resetMutation.mutate()
          }
        )
      ] })
    ] })
  ] }) });
}
function HostPanel({
  host,
  onClose,
  canWrite
}) {
  const [section, setSection] = useState("apps");
  const tabsId = useId();
  return /* @__PURE__ */ jsxs("div", { className: "sunshine-host-panel", children: [
    /* @__PURE__ */ jsxs("div", { className: "sunshine-panel-nav-row", children: [
      /* @__PURE__ */ jsx("nav", { className: "sunshine-subnav-inline", role: "tablist", "aria-label": `${host.name} \u7BA1\u7406\u529F\u80FD`, children: HOST_SECTIONS.map(({ key, label, Icon: Icon2 }, index) => /* @__PURE__ */ jsxs(
        "button",
        {
          type: "button",
          id: `${tabsId}-tab-${key}`,
          role: "tab",
          "aria-selected": section === key,
          "aria-controls": `${tabsId}-panel-${key}`,
          tabIndex: section === key ? 0 : -1,
          className: section === key ? "sunshine-section-tab active" : "sunshine-section-tab",
          onClick: () => setSection(key),
          onKeyDown: (event) => activateTabFromKeyboard(
            event,
            HOST_SECTIONS,
            index,
            (next) => setSection(next.key)
          ),
          children: [
            /* @__PURE__ */ jsx(Icon2, { size: 18 }),
            /* @__PURE__ */ jsx("strong", { children: label })
          ]
        },
        key
      )) }),
      /* @__PURE__ */ jsx(
        "button",
        {
          type: "button",
          className: "icon-button sunshine-panel-close",
          "aria-label": "\u5173\u95ED\u7BA1\u7406\u9762\u677F",
          title: "\u5173\u95ED",
          autoFocus: true,
          onClick: onClose,
          children: /* @__PURE__ */ jsx(X, { size: 18, "aria-hidden": "true" })
        }
      )
    ] }),
    HOST_SECTIONS.map(({ key }) => /* @__PURE__ */ jsxs(
      "div",
      {
        role: "tabpanel",
        id: `${tabsId}-panel-${key}`,
        "aria-labelledby": `${tabsId}-tab-${key}`,
        hidden: section !== key,
        children: [
          section === "apps" && key === "apps" ? /* @__PURE__ */ jsx(AppsSection, { host, canWrite }) : null,
          section === "clients" && key === "clients" ? /* @__PURE__ */ jsx(ClientsSection, { host, canWrite }) : null,
          section === "pairing" && key === "pairing" ? /* @__PURE__ */ jsx(PairingSection, { host, canWrite }) : null,
          section === "config" && key === "config" ? /* @__PURE__ */ jsx(ConfigSection, { host, canWrite }) : null,
          section === "system" && key === "system" ? /* @__PURE__ */ jsx(SystemSection, { host, canWrite }) : null
        ]
      },
      key
    ))
  ] });
}

// src/features/sunshine/SunshineView.tsx
function SunshineView({
  addTrigger = 0,
  onAddTriggerHandled,
  canWrite = false,
  canProxy = false
}) {
  const queryClient = useQueryClient();
  const createInFlightRef = useRef(false);
  const deletingHostIdsRef = useRef(/* @__PURE__ */ new Set());
  const hostsQuery = useQuery({
    queryKey: sunshineQueryKeys.sunshine.hosts,
    queryFn: ({ signal }) => querySunshineHosts(queryClient, signal),
    refetchInterval: (query) => sunshineHostsRefetchInterval(
      query.state.data,
      deletingHostIdsRef.current.size > 0
    )
  });
  const hosts = hostsQuery.data ?? [];
  const [selectedId, setSelectedId] = useState(null);
  const handledAddTriggerRef = useRef(0);
  const hostGridRef = useRef(null);
  const managementPanelRef = useRef(null);
  const managementPanelOpenerRef = useRef(null);
  const restoreManagementFocusRef = useRef(false);
  const createMutation = useMutation({
    mutationKey: sunshineHostMutationKeys.create,
    mutationFn: (request) => sunshineApi.sunshineCreateHost(request),
    onMutate: async (request) => {
      await queryClient.cancelQueries({ queryKey: sunshineQueryKeys.sunshine.hosts, exact: true });
      const optimistic = optimisticSunshineHost(request);
      queryClient.setQueryData(sunshineQueryKeys.sunshine.hosts, (current) => [
        ...current ?? [],
        optimistic
      ]);
      return { optimisticId: optimistic.id };
    },
    onSuccess: (saved, _request, context) => {
      queryClient.setQueryData(sunshineQueryKeys.sunshine.hosts, (current) => replaceSunshineHost(current ?? [], saved, context.optimisticId));
    },
    onError: (_error, _request, context) => {
      if (!context) return;
      queryClient.setQueryData(sunshineQueryKeys.sunshine.hosts, (current) => removeSunshineHost(current ?? [], context.optimisticId));
    },
    onSettled: () => {
      createInFlightRef.current = false;
      void queryClient.invalidateQueries({ queryKey: sunshineQueryKeys.sunshine.hosts, exact: true });
    }
  });
  const updateMutation = useMutation({
    mutationKey: sunshineHostMutationKeys.update,
    mutationFn: ({ id, patch }) => sunshineApi.sunshineUpdateHost(id, patch),
    onMutate: async ({ id, patch }) => {
      await queryClient.cancelQueries({ queryKey: sunshineQueryKeys.sunshine.hosts, exact: true });
      const previous = queryClient.getQueryData(sunshineQueryKeys.sunshine.hosts)?.find((host) => host.id === id);
      if (previous) {
        queryClient.setQueryData(sunshineQueryKeys.sunshine.hosts, (current) => replaceSunshineHost(current ?? [], applySunshineHostPatch(previous, patch)));
      }
      return { previous };
    },
    onSuccess: (saved) => {
      queryClient.setQueryData(sunshineQueryKeys.sunshine.hosts, (current) => replaceSunshineHost(current ?? [], saved));
    },
    onError: (_error, { id }, context) => {
      if (!context?.previous) return;
      queryClient.setQueryData(sunshineQueryKeys.sunshine.hosts, (current) => replaceSunshineHost(current ?? [], context.previous, id));
    },
    onSettled: (_result, _error, variables) => {
      if (Object.hasOwn(variables.patch, "password")) delete variables.patch.password;
      void queryClient.invalidateQueries({ queryKey: sunshineQueryKeys.sunshine.hosts, exact: true });
    }
  });
  const pendingUpdates = useMutationState({
    filters: {
      mutationKey: sunshineHostMutationKeys.update,
      exact: true,
      status: "pending"
    },
    select: (mutation) => mutation.state.variables
  });
  const updatingHostIds = new Set(pendingUpdates.map(({ id }) => id));
  const deleteMutation = useMutation({
    mutationKey: sunshineHostMutationKeys.delete,
    mutationFn: (id) => sunshineApi.sunshineDeleteHost(id),
    onMutate: async (id) => {
      await queryClient.cancelQueries({ queryKey: sunshineQueryKeys.sunshine.hosts, exact: true });
      const current = queryClient.getQueryData(sunshineQueryKeys.sunshine.hosts) ?? [];
      const originalIndex = current.findIndex((host) => host.id === id);
      const removed = originalIndex >= 0 ? current[originalIndex] : void 0;
      queryClient.setQueryData(
        sunshineQueryKeys.sunshine.hosts,
        removeSunshineHost(current, id)
      );
      if (selectedId === id) setSelectedId(null);
      return { originalIndex, removed };
    },
    onSuccess: (_result, id) => {
      queryClient.setQueryData(sunshineQueryKeys.sunshine.hosts, (current) => removeSunshineHost(current ?? [], id));
      queryClient.removeQueries({ queryKey: sunshineQueryKeys.sunshine.apps(id), exact: true });
      queryClient.removeQueries({ queryKey: sunshineQueryKeys.sunshine.clients(id), exact: true });
      queryClient.removeQueries({ queryKey: sunshineQueryKeys.sunshine.config(id), exact: true });
      queryClient.removeQueries({ queryKey: sunshineQueryKeys.logs.sunshine(id), exact: true });
    },
    onError: (_error, _id, context) => {
      if (!context?.removed) return;
      queryClient.setQueryData(sunshineQueryKeys.sunshine.hosts, (current) => restoreSunshineHost(current ?? [], context.removed, context.originalIndex));
    },
    onSettled: (_result, _error, id) => {
      deletingHostIdsRef.current.delete(id);
      void queryClient.invalidateQueries({ queryKey: sunshineQueryKeys.sunshine.hosts, exact: true });
    }
  });
  const selectedHost = hosts.find((host) => host.id === selectedId) ?? null;
  const closeManagementPanel = useCallback(() => {
    restoreManagementFocusRef.current = true;
    setSelectedId(null);
  }, []);
  useEffect(() => {
    if (!selectedHost) return;
    const closeOnEscape = (event) => {
      if (event.key === "Escape") closeManagementPanel();
    };
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [closeManagementPanel, selectedHost]);
  useLayoutEffect(() => {
    if (selectedHost || !restoreManagementFocusRef.current) return;
    restoreManagementFocusRef.current = false;
    const opener = managementPanelOpenerRef.current;
    managementPanelOpenerRef.current = null;
    if (opener?.isConnected && !opener.disabled) opener.focus();
  }, [selectedHost]);
  useLayoutEffect(() => {
    if (!selectedHost) return;
    const grid = hostGridRef.current;
    const panel = managementPanelRef.current;
    const selectedCard = grid?.querySelector(".sunshine-host-card.active");
    if (!grid || !panel || !selectedCard) return;
    const updatePosition = () => {
      const cards = Array.from(grid.querySelectorAll(".sunshine-host-card"));
      const selectedIndex = cards.indexOf(selectedCard);
      if (selectedIndex < 0) return;
      const gridStyle = window.getComputedStyle(grid);
      const columnCount = Math.max(1, gridStyle.gridTemplateColumns.split(/\s+/).filter(Boolean).length);
      const cardRect = selectedCard.getBoundingClientRect();
      const gridRect = grid.getBoundingClientRect();
      const layout = adjacentPanelLayout({
        cardWidth: cardRect.width,
        cardHeight: cardRect.height,
        columnGap: Number.parseFloat(gridStyle.columnGap) || 0,
        rowGap: Number.parseFloat(gridStyle.rowGap) || 0,
        column: selectedIndex % columnCount,
        columnCount,
        top: cardRect.top - gridRect.top
      });
      panel.style.left = `${layout.left}px`;
      panel.style.top = `${layout.top}px`;
      panel.style.width = `${layout.width}px`;
      panel.style.height = `${layout.height}px`;
      panel.style.borderRadius = `${cardRect.width / 18}px / ${cardRect.height / 12}px`;
      panel.dataset.placement = layout.placement;
      panel.style.visibility = "visible";
    };
    updatePosition();
    if (typeof ResizeObserver === "undefined") {
      window.addEventListener("resize", updatePosition);
      return () => window.removeEventListener("resize", updatePosition);
    }
    const resizeObserver = new ResizeObserver(updatePosition);
    resizeObserver.observe(grid);
    resizeObserver.observe(selectedCard);
    return () => resizeObserver.disconnect();
  }, [selectedHost]);
  function createDefaultHost() {
    if (!canWrite || createInFlightRef.current) return;
    const usedNames = new Set(hosts.map((host) => host.name));
    let index = hosts.length + 1;
    while (usedNames.has(`Sunshine ${index}`)) index += 1;
    createInFlightRef.current = true;
    createMutation.mutate({
      name: `Sunshine ${index}`,
      host: "192.168.1.2",
      web_port: 47990,
      username: "admin",
      password: null,
      verify_tls: true
    });
    setSelectedId(null);
  }
  useEffect(() => {
    if (addTrigger <= handledAddTriggerRef.current) return;
    if (!canWrite) {
      handledAddTriggerRef.current = addTrigger;
      onAddTriggerHandled?.(addTrigger);
      return;
    }
    if (createInFlightRef.current || createMutation.isPending) return;
    handledAddTriggerRef.current = addTrigger;
    onAddTriggerHandled?.(addTrigger);
    createDefaultHost();
  }, [addTrigger, canWrite, createMutation.isPending, onAddTriggerHandled]);
  function deleteHost(id) {
    if (deletingHostIdsRef.current.has(id)) return;
    deletingHostIdsRef.current.add(id);
    deleteMutation.mutate(id);
  }
  return /* @__PURE__ */ jsx("section", { className: "view-stack", children: /* @__PURE__ */ jsxs("section", { className: "section-band sunshine-new-section", children: [
    /* @__PURE__ */ jsx(MutationError, { mutation: createMutation }),
    /* @__PURE__ */ jsx(MutationError, { mutation: updateMutation }),
    /* @__PURE__ */ jsx(MutationError, { mutation: deleteMutation }),
    hostsQuery.error ? /* @__PURE__ */ jsx(InlineNotice, { tone: "danger", text: hostsQuery.error.message }) : null,
    hostsQuery.isLoading ? /* @__PURE__ */ jsx(LoadingBlock, { label: "\u8BFB\u53D6\u4E3B\u673A" }) : null,
    /* @__PURE__ */ jsx("div", { className: "instance-list-title", children: /* @__PURE__ */ jsx(ContentTitle, { icon: Boxes, title: "\u5B9E\u4F8B" }) }),
    /* @__PURE__ */ jsxs("div", { className: "sunshine-master-detail", children: [
      /* @__PURE__ */ jsx("div", { className: "sarmg-grid sunshine-host-grid", ref: hostGridRef, children: hosts.map((host) => /* @__PURE__ */ jsx(
        HostCard,
        {
          host,
          selected: selectedId === host.id,
          updating: updatingHostIds.has(host.id),
          canWrite,
          canManage: canProxy,
          onOpen: (trigger) => {
            if (isOptimisticSunshineHost(host)) return;
            if (selectedId === host.id) {
              closeManagementPanel();
              return;
            }
            managementPanelOpenerRef.current = trigger;
            restoreManagementFocusRef.current = false;
            setSelectedId(host.id);
          },
          onInlineUpdate: (patch) => updateMutation.mutateAsync({ id: host.id, patch }).then(() => void 0),
          onDelete: () => {
            if (isOptimisticSunshineHost(host)) return;
            if (window.confirm(`\u786E\u5B9A\u5220\u9664\u4E3B\u673A "${host.name}"\uFF1F`)) deleteHost(host.id);
          }
        },
        host.id
      )) }),
      selectedHost ? /* @__PURE__ */ jsx(
        "aside",
        {
          ref: managementPanelRef,
          className: "sunshine-adj-panel",
          role: "dialog",
          "aria-label": `${selectedHost.name} \u7BA1\u7406\u9762\u677F`,
          children: /* @__PURE__ */ jsx(
            HostPanel,
            {
              host: selectedHost,
              onClose: closeManagementPanel,
              canWrite
            },
            selectedHost.id
          )
        }
      ) : null
    ] })
  ] }) });
}

// src/features/logs/LogViewer.tsx
function LogViewer({
  logs,
  loading
}) {
  return /* @__PURE__ */ jsxs("div", { className: "log-viewer", children: [
    /* @__PURE__ */ jsxs("div", { className: "log-toolbar", children: [
      /* @__PURE__ */ jsx("span", { children: logs?.path ?? "\u7B49\u5F85\u65E5\u5FD7\u6587\u4EF6" }),
      /* @__PURE__ */ jsxs("span", { children: [
        logs?.lines.length ?? 0,
        " \u884C"
      ] })
    ] }),
    /* @__PURE__ */ jsx("pre", { children: loading ? "loading..." : logs?.lines.length ? logs.lines.join("\n") : "\u6682\u65E0\u65E5\u5FD7" })
  ] });
}

// src/features/logs/LogsView.tsx
var MAX_RENDERED_LOG_LINES = 2e3;
function limitLogLines(lines, limit = MAX_RENDERED_LOG_LINES) {
  if (lines.length <= limit) return [...lines];
  return [`\u2026 \u5DF2\u7701\u7565\u524D ${lines.length - limit} \u884C\uFF0C\u4EC5\u663E\u793A\u6700\u65B0 ${limit} \u884C`, ...lines.slice(-limit)];
}
function LogsView() {
  const queryClient = useQueryClient();
  const [preferredHostId, setPreferredHostId] = useState(null);
  const hostsQuery = useQuery({
    queryKey: sunshineQueryKeys.sunshine.hosts,
    queryFn: ({ signal }) => querySunshineHosts(queryClient, signal),
    refetchInterval: (query) => sunshineHostsRefetchInterval(query.state.data)
  });
  const hosts = persistedSunshineHosts(hostsQuery.data ?? []);
  const selectedHostId = preferredHostId && hosts.some((host) => host.id === preferredHostId) ? preferredHostId : hosts[0]?.id ?? null;
  const logsQuery = useQuery({
    queryKey: sunshineQueryKeys.logs.sunshine(selectedHostId ?? ""),
    queryFn: () => sunshineApi.sunshineApiLogs(selectedHostId),
    enabled: Boolean(selectedHostId),
    refetchInterval: 3e4
  });
  const selectedHost = hosts.find((host) => host.id === selectedHostId);
  const logLines = useMemo(
    () => logsQuery.data === void 0 ? void 0 : limitLogLines(sunshineLogLines(logsQuery.data)),
    [logsQuery.data]
  );
  const logs = logLines && selectedHost ? { path: `Sunshine API \xB7 ${selectedHost.name}`, lines: logLines } : void 0;
  return /* @__PURE__ */ jsx("section", { className: "view-stack logs-view-stack", children: /* @__PURE__ */ jsxs("section", { className: "section-band", children: [
    /* @__PURE__ */ jsx(SectionHeader, { icon: Terminal, title: "\u65E5\u5FD7" }),
    hostsQuery.isLoading ? /* @__PURE__ */ jsx(LoadingBlock, { label: "\u8BFB\u53D6\u4E3B\u673A\u5217\u8868" }) : null,
    hostsQuery.error ? /* @__PURE__ */ jsx(InlineNotice, { tone: "danger", text: `\u4E3B\u673A\u5217\u8868\u8BFB\u53D6\u5931\u8D25\uFF1A${hostsQuery.error.message}` }) : null,
    !hostsQuery.isLoading && !hostsQuery.error && !hosts.length ? /* @__PURE__ */ jsx(InlineNotice, { tone: "warn", text: "\u6682\u65E0\u5DF2\u914D\u7F6E\u7684 Sunshine \u4E3B\u673A" }) : null,
    hosts.length ? /* @__PURE__ */ jsxs("label", { className: "logs-host-selector", children: [
      /* @__PURE__ */ jsx("span", { children: "\u4E3B\u673A" }),
      /* @__PURE__ */ jsx("select", { value: selectedHostId ?? "", onChange: (event) => setPreferredHostId(event.target.value), children: hosts.map((host) => /* @__PURE__ */ jsx("option", { value: host.id, children: host.name }, host.id)) })
    ] }) : null,
    logsQuery.error ? /* @__PURE__ */ jsx(InlineNotice, { tone: "danger", text: `\u65E5\u5FD7\u8BFB\u53D6\u5931\u8D25\uFF1A${logsQuery.error.message}` }) : null,
    selectedHost ? /* @__PURE__ */ jsx(LogViewer, { logs, loading: logsQuery.isLoading }) : null
  ] }) });
}

// src/app.tsx
function activate() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { refetchOnWindowFocus: false, retry: 1, staleTime: 5e3 },
      mutations: { retry: false }
    }
  });
  function SunshineView2(props) {
    return /* @__PURE__ */ jsx(QueryClientProvider, { client: queryClient, children: /* @__PURE__ */ jsx(
      SunshineView,
      {
        addTrigger: props.actionRequest,
        onAddTriggerHandled: props.onActionRequestHandled,
        canWrite: props.hasPermission("sunshine.hosts.write"),
        canProxy: props.hasPermission("sunshine.proxy.use")
      }
    ) });
  }
  function SunshineLogsView() {
    return /* @__PURE__ */ jsx(QueryClientProvider, { client: queryClient, children: /* @__PURE__ */ jsx(LogsView, {}) });
  }
  return {
    components: { SunshineView: SunshineView2, SunshineLogsView },
    primaryActions: [{
      component: "SunshineView",
      label: "\u6DFB\u52A0 Sunshine \u4E3B\u673A",
      permission: "sunshine.hosts.write"
    }]
  };
}
export {
  activate
};
/*! Bundled license information:

lucide-react/dist/esm/shared/src/utils/mergeClasses.mjs:
lucide-react/dist/esm/shared/src/utils/toKebabCase.mjs:
lucide-react/dist/esm/shared/src/utils/toCamelCase.mjs:
lucide-react/dist/esm/shared/src/utils/toPascalCase.mjs:
lucide-react/dist/esm/defaultAttributes.mjs:
lucide-react/dist/esm/shared/src/utils/hasA11yProp.mjs:
lucide-react/dist/esm/context.mjs:
lucide-react/dist/esm/Icon.mjs:
lucide-react/dist/esm/createLucideIcon.mjs:
lucide-react/dist/esm/icons/app-window.mjs:
lucide-react/dist/esm/icons/bell-dot.mjs:
lucide-react/dist/esm/icons/boxes.mjs:
lucide-react/dist/esm/icons/check.mjs:
lucide-react/dist/esm/icons/external-link.mjs:
lucide-react/dist/esm/icons/key-round.mjs:
lucide-react/dist/esm/icons/loader-circle.mjs:
lucide-react/dist/esm/icons/pen.mjs:
lucide-react/dist/esm/icons/plus.mjs:
lucide-react/dist/esm/icons/refresh-cw.mjs:
lucide-react/dist/esm/icons/rotate-ccw.mjs:
lucide-react/dist/esm/icons/settings-2.mjs:
lucide-react/dist/esm/icons/terminal.mjs:
lucide-react/dist/esm/icons/toggle-left.mjs:
lucide-react/dist/esm/icons/toggle-right.mjs:
lucide-react/dist/esm/icons/trash-2.mjs:
lucide-react/dist/esm/icons/unlink.mjs:
lucide-react/dist/esm/icons/users.mjs:
lucide-react/dist/esm/icons/wrench.mjs:
lucide-react/dist/esm/icons/x.mjs:
lucide-react/dist/esm/lucide-react.mjs:
  (**
   * @license lucide-react v1.35.0 - ISC
   *
   * This source code is licensed under the ISC license.
   * See the LICENSE file in the root directory of this source tree.
   *)
*/
