/* Quick Achieve 前端垫层：把 window.__TAURI__ 用 IPC 桥接回 Rust。
 *
 * 前端只用到三样东西（已核实）：
 *   · __TAURI__.core.invoke(cmd, args)
 *   · __TAURI__.event.listen(ev, cb)
 *   · __TAURI__.window.getCurrentWindow()
 *
 * 与原 Tauri 的差异：这里没有 Tauri 的 JS 运行时，全部由 Rust 侧
 * 自定义协议 + postMessage 实现。语义保持一致（invoke 返回 Promise）。
 */
(function () {
  if (window.__TAURI__ && window.__TAURI__.__qs) return;

  var pending = new Map();
  var seq = 0;
  var listeners = Object.create(null);
  var LABEL = '__QS_LABEL__';

  window.__qsIpc = {
    resolve: function (id, ok, v) {
      var p = pending.get(id);
      if (!p) return;
      pending.delete(id);
      if (ok) {
        if (v && typeof v === 'object' && v.__qsRaw) {
          p.res(__qsB64ToBuf(v.__qsRaw));
        } else {
          p.res(v);
        }
      } else {
        var msg = typeof v === 'string' ? v : JSON.stringify(v);
        /* v2.23.15：命令失败**一定**在 log.txt 里留一行。
         *
         * 界面上有近百处 `invoke(...)` 是"发了就不管"的（点了按钮没反应
         * 那种），后端返回 Err 时前端只会产生一个未处理的 Promise rejection ——
         * 控制台里都看不到，用户那边就是"点了没反应"，事后完全没法查。
         * 这里顺手把「哪个命令、报了什么」写进日志（走同一条 IPC，不递归调用）。 */
        try {
          window.ipc.postMessage(
            JSON.stringify({
              id: ++seq,
              cmd: 'log_msg',
              args: { msg: '[ipc] 命令失败 ' + (p.cmd || '?') + ' → ' + msg },
            })
          );
        } catch (_) {}
        p.rej(new Error(msg));
      }
    },
  };

  window.__qsEmit = function (ev, payload) {
    var arr = listeners[ev];
    if (!arr) return;
    for (var i = 0; i < arr.length; i++) {
      try {
        arr[i]({ event: ev, id: 0, payload: payload });
      } catch (e) {
        console.error('[qs] listener error', ev, e);
      }
    }
  };

  function __qsB64ToBuf(s) {
    var bin = atob(s);
    var len = bin.length;
    var out = new Uint8Array(len);
    for (var i = 0; i < len; i++) out[i] = bin.charCodeAt(i);
    return out.buffer;
  }

  function invoke(cmd, args) {
    return new Promise(function (res, rej) {
      var id = ++seq;
      // 记下命令名：失败时要用它写日志（见上面 __qsIpc.resolve 的说明）
      pending.set(id, { res: res, rej: rej, cmd: cmd });
      try {
        window.ipc.postMessage(
          JSON.stringify({ id: id, cmd: cmd, args: args || {} })
        );
      } catch (e) {
        pending.delete(id);
        rej(e);
      }
    });
  }

  function listen(ev, cb) {
    (listeners[ev] || (listeners[ev] = [])).push(cb);
    return Promise.resolve(function () {
      var a = listeners[ev] || [];
      var i = a.indexOf(cb);
      if (i >= 0) a.splice(i, 1);
    });
  }

  /* 窗口代理：Tauri v2 里这些方法都是 Promise，这里保持一致。
     真正的窗口操作走对应的 Rust 命令（__qsWindowOp）。 */
  function winProxy() {
    function op(name, arg) {
      return invoke('__qs_window_op', { label: LABEL, op: name, arg: arg });
    }
    return {
      label: LABEL,
      setTitle: function (t) { return op('setTitle', t); },
      show: function () { return op('show'); },
      hide: function () { return op('hide'); },
      close: function () { return op('close'); },
      destroy: function () { return op('destroy'); },
      setFocus: function () { return op('setFocus'); },
      isVisible: function () { return op('isVisible').then(function (v) { return !!v; }); },
      minimize: function () { return op('minimize'); },
      unminimize: function () { return op('unminimize'); },
      setAlwaysOnTop: function (v) { return op('setAlwaysOnTop', v); },
      setSize: function () { return Promise.resolve(); },
      innerSize: function () { return op('innerSize'); },
      scaleFactor: function () { return Promise.resolve(1); },
      onCloseRequested: function () { return Promise.resolve(function () {}); },
      once: function () {},
    };
  }

  var W = winProxy();
  window.__TAURI__ = {
    __qs: true,
    core: { invoke: invoke },
    event: {
      listen: listen,
      once: function (ev, cb) { return listen(ev, cb); },
      emit: function (ev, payload) {
        try {
          window.ipc.postMessage(
            JSON.stringify({ kind: 'emit', event: ev, payload: payload })
          );
        } catch (e) { /* ignore */ }
        return Promise.resolve();
      },
      emitTo: function (_t, ev, payload) {
        return window.__TAURI__.event.emit(ev, payload);
      },
    },
    window: {
      getCurrentWindow: function () { return W; },
      getCurrentWebviewWindow: function () { return W; },
      getAllWindows: function () { return Promise.resolve([W]); },
    },
    webviewWindow: {
      getCurrentWebviewWindow: function () { return W; },
    },
    app: {
      version: function () { return Promise.resolve('2.0.0'); },
      getName: function () { return Promise.resolve('Quick Achieve'); },
    },
  };
})();
