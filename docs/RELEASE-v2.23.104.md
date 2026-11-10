# Quick Achieve v2.23.104

> 收口版本：把 E2E 的**语法 + 场景清单**纳入 CI 与发版脚本，
> 并补一份七项优化验收文档。

## 一、E2E 进 CI（不需要桌面会话的部分）

真正跑 E2E 需要 Windows 桌面会话（WebView2 + 真实指针），GitHub runner
上跑不了；但“场景文件写坏了 / `run.js` 登记漏了”可以在 CI 里静态拦住。

`.github/workflows/ci.yml` 新增：

```yaml
- name: E2E 场景语法与清单（不需要桌面会话）
  shell: pwsh
  run: |
    Get-ChildItem tools/e2e/*.js | ForEach-Object {
      node --check $_.FullName
      if ($LASTEXITCODE -ne 0) { throw "语法错误：$($_.Name)" }
    }
    node tools/e2e/run.js --list
```

`tools/release.ps1` 的第 2 步也加了同一道检查，发版前本地同样会拦。

## 二、验收文档

新增 [ACCEPTANCE-2026-11-10.md](D:\Local AI\git\Quick Achieve 源代码\docs\ACCEPTANCE-2026-11-10.md)：

- 七项逐条证据（搜索/恢复默认、schema 对账、面板按需渲染、图标 WebP、
  线程收敛、E2E + CI、拆分）；
- 前后指标：exe 8.20 → 7.61 MB、UI 5.17 → 3.44 MB、main −769 行、
  E2E 0 → 4 场景 31 断言；
- 最终验证命令与结果。

## 三、验证

| 项目 | 结果 |
|---|---|
| `cargo test` | 474 passed / 0 failed / 2 ignored |
| `cargo clippy --all-targets -- -D warnings` | **通过** |
| `tools/test-ui.js` | 887 / 887 |
| `tools/test-pages.js` | 56 / 56 |
| `tools/check-all.js` | 五项全过 |
| E2E 语法 + `--list` | 4 个场景，全部通过 |
| `node tools/e2e/run.js` | **4/4 场景、31 断言** |

---

**SHA256: 0000000000000000000000000000000000000000000000000000000000000000**
