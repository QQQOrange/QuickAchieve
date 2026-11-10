# Quick Achieve 一键发版：构建 → 测试 → 打 tag → 建 Release → 传资产 → 校验
#
# 用法：
#   pwsh tools/release.ps1 -Version 2.23.6 -DryRun    # 只构建 + 写 hash，不推送
#   pwsh tools/release.ps1 -Version 2.23.6            # 完整发版
#
# 顺序本身有讲究，别随意调：
#   1. 工作区必须干净、分支必须是 main —— 否则发的可能是半截代码；
#   2. Cargo.toml / app.rc 的版本号必须等于 -Version —— 免得 exe 里是旧版本号；
#   3. 三套测试（Rust + 两个 jsdom 套件）任一不过就不发；
#   4. 构建 release，算 SHA256；
#   5. 把 SHA256 写进 docs/RELEASE-v<ver>.md（自动更新器靠这行核对下载到的 exe）；
#   6. 提交 + push main + 打 annotated tag + push tag；
#   7. 建 Release、传 exe、回读校验（大小 / digest / 说明里的 SHA256 三者对齐）。
param(
    [Parameter(Mandatory = $true)][string]$Version,
    [switch]$DryRun
)

# ⚠ 默认 Continue 而不是 Stop：cargo/rustc 会把进度写到 stderr，PowerShell 5.1
#   在 Stop 下会把那条 stderr 当成终止错误，构建脚本直接崩在这里。
#   关键步骤一律用 $LASTEXITCODE / try-catch 显式判（见下面各步）。
$ErrorActionPreference = 'Continue'
$Repo = Split-Path -Parent $PSScriptRoot
$Tag = "v$Version"
$Exe = Join-Path $Repo 'target\release\QuickAchieve.exe'
$NotesRel = "docs/RELEASE-$Tag.md"
$Notes = Join-Path $Repo ($NotesRel -replace '/', '\')

function Step($m) { Write-Host "`n=== $m ===" -ForegroundColor Cyan }
function Ok($m) { Write-Host "[OK] $m" -ForegroundColor Green }
function Fail($m) { Write-Host "[FAIL] $m" -ForegroundColor Red; exit 1 }

function Get-Token {
    $cred = ("protocol=https`nhost=github.com`n`n" | git credential fill) -split "`n"
    $tok = ($cred | Where-Object { $_ -match '^password=' }) -replace '^password=', ''
    if (-not $tok) { Fail '拿不到 GitHub token（git credential fill 返回空）' }
    return $tok
}

Push-Location $Repo
try {
    Step "1/7 前置检查 $Tag"
    if (git status --porcelain) { Fail '工作区不干净：先提交或 stash' }
    $branch = (git rev-parse --abbrev-ref HEAD).Trim()
    if ($branch -ne 'main') { Fail "当前分支是 $branch，只允许从 main 发版" }
    Ok '工作区干净，分支 main'

    $m = Select-String -Path 'Cargo.toml' -Pattern '^version = "([0-9.]+)"' | Select-Object -First 1
    $cargoVer = $m.Matches[0].Groups[1].Value
    if ($cargoVer -ne $Version) { Fail "Cargo.toml 是 $cargoVer，与 -Version $Version 不一致" }
    $minor = $Version.Split('.')[-1]
    if (-not (Select-String -Path 'app.rc' -Pattern "FILEVERSION 2,23,$minor,0" -Quiet)) {
        Fail "app.rc 的 FILEVERSION 没跟到 $Version"
    }
    if (-not (Test-Path $Notes)) { Fail "缺少发布说明 $NotesRel（先写好，里面留一行 **SHA256: ...**）" }
    Ok "版本号一致（Cargo.toml / app.rc = $Version），发布说明存在"

    Step '2/7 测试'
    $env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
    # v2.23.61：**串行跑**。剪贴板相关的用例会互相抢剪贴板（并行时谁先谁后不定），
# 以前偶尔会在发版这一步偶发红一次；串行后多花十几秒，但结果确定。
$rt = cargo test -- --test-threads=1 2>&1 | Select-String -Pattern '^test result:'
    if ($LASTEXITCODE -ne 0) { Fail 'cargo test 退出码非 0' }
    if (-not $rt -or ($rt -join ' ') -notmatch 'ok\.') { Fail 'cargo test 没有 "ok." 结果行' }
    Ok ('cargo test  ' + ($rt -join ' | '))

    if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
        $fb = 'C:\Users\64381\.dsh\dsh-runtimes\dsh-primary-runtime\dependencies\node\bin'
        if (Test-Path $fb) { $env:PATH = "$fb;$env:PATH" }
    }
    if (-not (Get-Command node -ErrorAction SilentlyContinue)) { Fail '找不到 node' }
    # v2.23.15：发布前顺手校验「前端 invoke 的命令后端都有」——
    # 缺一个就是界面上有个按钮点了不会动，这种 bug 靠人眼很难发现。
    $cmdMap = node tools/check-cmd-map.js 2>&1 | Out-String
    if ($LASTEXITCODE -ne 0) { Fail 'check-cmd-map.js：有前端调用找不到后端命令' }
    Write-Host '    [OK] 命令表一致'

    foreach ($suite in @('tools/test-ui.js', 'tools/test-pages.js')) {
        $out = node $suite 2>&1 | Select-String -Pattern '通过|存在失败'
        if ($LASTEXITCODE -ne 0) { Fail "$suite 退出码非 0" }
        # ⚠ 判套件自己打印的结论行，不搜 'FAIL'：PowerShell 的 -match 默认
        #   忽略大小写，而测试名里有 "fail-safe"，用 FAIL 会把它当成失败项。
        if ("$out" -match '存在失败') { Fail "$suite 有失败项" }
        if ("$out" -notmatch '全部通过') { Fail "$suite 没有打印 全部通过" }
        Ok ("$suite  " + ($out | Select-Object -Last 1))
    }

    # v2.23.41：**英文覆盖也进发布门槛**。
    # 原来它只是"我偶尔手动跑一次"—— 而漏跑一次就等于把没翻译的中文带进版本
    # （v2.23.36 就是这么漏的：整块「复制诊断信息」在隐藏分页里，没人发现）。
    # 允许的残留只有三类应当保留中文的（语言自称 / 引擎名），阈值 3 写在
    # tools/check-all.js 里，超了直接拦住发布。
    $cov = node tools/check-all.js --quiet 2>&1 | Out-String
    if ($LASTEXITCODE -ne 0) {
        Write-Host $cov
        Fail 'tools/check-all.js：前端自检有失败项（含英文覆盖超阈值）'
    }
    Ok '前端五项自检（UI / 页面 / 命令表 / 英文覆盖 / 设置项）全部通过'

    # v2.23.104：E2E 场景语法/清单。真机跑（node tools/e2e/run.js）仍要桌面会话，
    # 这里只做不依赖 GUI 的静态检查，保证场景文件没写坏、run.js 登记没漏。
    Get-ChildItem tools/e2e/*.js | ForEach-Object {
        node --check $_.FullName 2>&1 | Out-Null
        if ($LASTEXITCODE -ne 0) { Fail "E2E 语法错误：$($_.Name)" }
    }
    node tools/e2e/run.js --list 2>&1 | Out-Null
    if ($LASTEXITCODE -ne 0) { Fail 'tools/e2e/run.js --list 失败（场景登记有误）' }
    Ok 'E2E 场景语法与清单（4 个场景）'

    Step '3/7 构建 release'
    $env:ORT_SKIP_DOWNLOAD = '1'
    cargo build --release --message-format=short 2>&1 |
        Select-String -Pattern '^error|Finished' | ForEach-Object { Write-Host $_.Line }
    if (-not (Test-Path $Exe)) { Fail '构建产物不存在' }
    $file = Get-Item $Exe
    if ($file.VersionInfo.FileVersion -ne $Version) {
        Fail "产物版本号是 $($file.VersionInfo.FileVersion)，不是 $Version"
    }
    $sha = (Get-FileHash $Exe -Algorithm SHA256).Hash
    $size = $file.Length
    Ok ("产物 {0}  {1:N0} B  sha256 {2}..." -f $file.VersionInfo.FileVersion, $size, $sha.Substring(0, 12))

    Step '4/7 把 SHA256 写进发布说明'
    $text = [System.IO.File]::ReadAllText($Notes, [System.Text.Encoding]::UTF8)
    # ⚠ 行尾只吃空格/制表符（[ \t]*），不能用 \s* —— 那会把后面的换行一起吃掉，
    #   每跑一次少一个空行，发布说明的文件会越跑越瘪。
    $patched = [regex]::Replace($text, '(?m)^\*\*SHA256:[ \t]*[0-9A-Fa-f]{0,64}\*\*[ \t]*$',
        "**SHA256: $sha**")
    if ($patched -notmatch [regex]::Escape($sha)) {
        Fail "$NotesRel 里找不到 `"**SHA256: ...**`" 那一行，没法自动填"
    }
    if ($patched -ne $text) {
        [System.IO.File]::WriteAllText($Notes, $patched, (New-Object System.Text.UTF8Encoding $false))
        Ok "已更新 $NotesRel"
    } else {
        Ok '发布说明里的 SHA256 已经是对的'
    }

    if ($DryRun) {
        Step 'DryRun 到此为止'
        Ok "没有提交、没有推送。产物：$Exe"
        return
    }

    Step '5/7 提交 + push + tag'
    git add -A
    if (git status --porcelain) {
        git -c user.name=QQQOrange -c user.email=62794181+QQQOrange@users.noreply.github.com commit -m "release: $Tag" | Out-Null
        Ok '已提交发布说明'
    }
    $out = (git push origin main 2>&1 | Out-String)
    if ($out -notmatch 'main -> main') { Fail "push main 失败：$out" }
    Ok 'push main'
    git -c user.name=QQQOrange -c user.email=62794181+QQQOrange@users.noreply.github.com tag -a $Tag -m "Quick Achieve $Tag"
    $out = (git push origin $Tag 2>&1 | Out-String)
    if ($out -notmatch [regex]::Escape("$Tag -> $Tag")) { Fail "push tag 失败：$out" }
    Ok "push tag $Tag"

    Step '6/7 建 Release 并上传 exe'
    $h = @{ Authorization = "token $(Get-Token)"; Accept = 'application/vnd.github+json'; 'User-Agent' = 'QuickAchieve-Release' }
    $payload = @{ tag_name = $Tag; name = "Quick Achieve $Tag"; body = $patched; draft = $false; prerelease = $false } |
        ConvertTo-Json -Depth 5
    $bodyFile = Join-Path $env:TEMP "qa-release-$Tag.json"
    [System.IO.File]::WriteAllText($bodyFile, $payload, (New-Object System.Text.UTF8Encoding $false))
    try {
        $rel = Invoke-RestMethod -Method Post -Uri 'https://api.github.com/repos/QQQOrange/QuickAchieve/releases' `
            -Headers $h -InFile $bodyFile -ContentType 'application/json; charset=utf-8'
    } catch {
        Fail "建 Release 失败（tag 已经推上去了，修好后可以只补 Release）：$($_.Exception.Message)"
    }
    Ok ("Release id={0}" -f $rel.id)
    try {
        $asset = Invoke-RestMethod -Method Post `
            -Uri ("https://uploads.github.com/repos/QQQOrange/QuickAchieve/releases/{0}/assets?name=QuickAchieve.exe" -f $rel.id) `
            -Headers $h -InFile $Exe -ContentType 'application/octet-stream' -TimeoutSec 300
    } catch {
        Fail "上传资产失败：$($_.Exception.Message)"
    }
    Ok ("资产 {0}  {1:N0} B  {2}" -f $asset.name, $asset.size, $asset.state)

    Step '7/7 回读校验'
    try {
        $latest = Invoke-RestMethod -Uri 'https://api.github.com/repos/QQQOrange/QuickAchieve/releases/latest' `
            -Headers $h -TimeoutSec 20
    } catch {
        Fail "回读 releases/latest 失败：$($_.Exception.Message)"
    }
    if ($latest.tag_name -ne $Tag) { Fail "releases/latest 是 $($latest.tag_name)，不是 $Tag" }
    if ($latest.assets[0].size -ne $size) { Fail "资产大小 $($latest.assets[0].size) != 本地 $size" }
    if ($latest.assets[0].digest -ne ('sha256:' + $sha.ToLower())) {
        Fail "资产 digest $($latest.assets[0].digest) 与本地 $sha 不一致"
    }
    $mm = [regex]::Match($latest.body, 'SHA256:\s*([0-9A-Fa-f]{64})')
    if (-not $mm.Success -or $mm.Groups[1].Value.ToLower() -ne $sha.ToLower()) {
        Fail '发布说明里的 SHA256 与资产不一致'
    }
    Ok "latest=$Tag  大小一致  说明里的 SHA256 与资产 digest 一致"
    Write-Host ""
    Write-Host $latest.html_url -ForegroundColor Green
}
finally {
    Pop-Location
}
