'use strict';
/* ============================================================================
 * Quick Achieve —— 真机 E2E 小框架（v2.23.89）
 * ----------------------------------------------------------------------------
 * 为什么要有：
 *   这次排查「移远收起」「静态磨砂」「恢复本节默认」全靠临时写在 %TEMP% 里的
 *   CDP 探针。探针用完就丢，下次回归还得重写，而且没有人知道它们存在。
 *   把「启动隔离实例 → 连 CDP → 操作 → 断言」收成这一层，场景文件只写业务。
 *
 * 边界：
 *   · 只支持 Windows 桌面会话（要 WebView2 + 指针坐标）；
 *   · 必须先把 target/release/QuickAchieve.exe 构建出来；
 *   · 每个场景用自己的临时数据目录 + 独立 CDP 端口，**不碰**真实配置；
 *   · 测试进程带 QS_NO_ELEVATE=1，正常不会被提权；清理只杀
 *     target/release 下这一份 exe，绝不碰 D:\software\myself 的正式版。
 *
 * 用法见 tools/e2e/README.md。
 * ========================================================================== */

const { spawn, execFileSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

const ROOT = path.resolve(__dirname, '..', '..');
const EXE = path.join(ROOT, 'target', 'release', 'QuickAchieve.exe');

const sleep = (ms) => Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);

function ps(command) {
    return execFileSync('powershell.exe', ['-NoProfile', '-Command', command], {
        encoding: 'utf8',
        windowsHide: true,
    }).trim();
}

function setCursor(x, y) {
    ps('Add-Type -AssemblyName System.Windows.Forms;'
        + `[System.Windows.Forms.Cursor]::Position = New-Object System.Drawing.Point(${Math.round(x)},${Math.round(y)})`);
}

function cursorPos() {
    const out = ps('Add-Type -AssemblyName System.Windows.Forms;'
        + '$p=[System.Windows.Forms.Cursor]::Position; "$($p.X),$($p.Y)"');
    const [x, y] = out.split(',').map((v) => Number(v.trim()));
    return { x, y };
}

/** 设置指针并确认真的到位（远程/被别的程序抢焦点时 SetCursorPos 可能不生效）。 */
function setCursorExact(x, y) {
    for (let i = 0; i < 5; i++) {
        setCursor(x, y);
        sleep(60);
        const p = cursorPos();
        if (Math.abs(p.x - x) <= 1 && Math.abs(p.y - y) <= 1) return true;
    }
    return false;
}

async function waitTarget(port, sub, timeoutMs = 15000) {
    const end = Date.now() + timeoutMs;
    while (Date.now() < end) {
        try {
            const r = await fetch(`http://127.0.0.1:${port}/json/list`);
            const list = await r.json();
            const hit = list.find((t) => (t.url || '').includes(sub));
            if (hit) return hit;
        } catch (_) { /* 还没起来 */ }
        sleep(250);
    }
    return null;
}

class Conn {
    constructor(ws) {
        this.ws = ws;
        this.seq = 0;
        this.open = new Promise((res, rej) => {
            ws.addEventListener('open', res, { once: true });
            ws.addEventListener('error', rej, { once: true });
        });
    }

    call(method, params) {
        return new Promise((res, rej) => {
            const id = ++this.seq;
            const on = (ev) => {
                let m;
                try { m = JSON.parse(ev.data); } catch { return; }
                if (m.id !== id) return;
                this.ws.removeEventListener('message', on);
                m.error ? rej(new Error(m.error.message)) : res(m.result);
            };
            this.ws.addEventListener('message', on);
            this.ws.send(JSON.stringify({ id, method, params: params || {} }));
        });
    }

    async eval(expression) {
        const r = await this.call('Runtime.evaluate', {
            expression,
            returnByValue: true,
            awaitPromise: true,
        });
        if (r.exceptionDetails) {
            const d = r.exceptionDetails.exception && r.exceptionDetails.exception.description;
            throw new Error(d || r.exceptionDetails.text || '页面异常');
        }
        return r.result.value;
    }

    invoke(cmd, args) {
        return this.eval(`window.__TAURI__.core.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args || {})})`);
    }

    close() { try { this.ws.close(); } catch (_) { } }
}

class App {
    constructor(opts) {
        this.tag = opts.tag;
        this.port = opts.port;
        this.dataDir = path.join(os.tmpdir(), `qa-e2e-${opts.tag}-${process.pid}`);
        this.pid = 0;
        this.started = Date.now();
    }

    async launch() {
        fs.rmSync(this.dataDir, { recursive: true, force: true });
        fs.mkdirSync(this.dataDir, { recursive: true });
        if (!fs.existsSync(EXE)) {
            throw new Error(`没有构建产物：${EXE}（先跑 cargo build --release）`);
        }
        const env = Object.assign({}, process.env, {
            QS_DATA_DIR: this.dataDir,
            QS_INSTANCE_TAG: `e2e-${this.tag}`,
            QS_NO_CLOUD: '1',
            QS_NO_ELEVATE: '1',
            QS_DEBUG: '1',
            QS_T_WV_ARGS: '--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection '
                + `--disable-gpu-sandbox --remote-debugging-port=${this.port}`,
        });
        const child = spawn(EXE, ['--no-elevate'], {
            cwd: ROOT,
            env,
            detached: true,
            stdio: 'ignore',
        });
        this.pid = child.pid;
        child.unref();
        const t = await waitTarget(this.port, 'index.html');
        if (!t) throw new Error(`等不到横条 WebView（端口 ${this.port}）`);
        await sleep(900);
        return t;
    }

    async connectBar() {
        const t = await waitTarget(this.port, 'index.html');
        if (!t) throw new Error('横条 target 不存在');
        const c = new Conn(new WebSocket(t.webSocketDebuggerUrl));
        await c.open;
        return c;
    }

    async connectSettings() {
        const t = await waitTarget(this.port, 'settings.html', 20000);
        if (!t) throw new Error('设置页 target 不存在（open_settings 没生效？）');
        const c = new Conn(new WebSocket(t.webSocketDebuggerUrl));
        await c.open;
        return c;
    }

    logTail(n = 200) {
        try {
            const p = path.join(this.dataDir, 'log.txt');
            return fs.readFileSync(p, 'utf8').split('\n').slice(-n).join('\n');
        } catch (_) { return ''; }
    }

    /** 只杀「target/release 这一份」的进程；正式版路径不同，不会被误伤。 */
    async stop() {
        try { execFileSync('taskkill', ['/PID', String(this.pid), '/T', '/F'], { stdio: 'ignore' }); } catch (_) { }
        try {
            ps(`Get-Process QuickAchieve -ErrorAction SilentlyContinue | `
                + `Where-Object { $_.Path -eq '${EXE.replace(/'/g, "''")}' } | `
                + `ForEach-Object { Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue }`);
        } catch (_) { }
        await sleep(300);
    }
}

class Suite {
    constructor(name) {
        this.name = name;
        this.pass = 0;
        this.fail = 0;
        console.log(`\n=== E2E · ${name} ===`);
    }

    skip(title, detail) {
        console.log(`SKIP  ${title}${detail ? `\n        ${detail}` : ''}`);
    }

    ok(title, cond, detail) {
        if (cond) {
            this.pass += 1;
            console.log(`PASS  ${title}`);
        } else {
            this.fail += 1;
            console.log(`FAIL  ${title}${detail !== undefined ? `\n        ${detail}` : ''}`);
        }
        return !!cond;
    }

    done() {
        console.log(`--- ${this.name}: ${this.pass} passed, ${this.fail} failed`);
        return this.fail === 0;
    }
}

module.exports = { ROOT, EXE, sleep, ps, setCursor, cursorPos, setCursorExact, waitTarget, Conn, App, Suite };
