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

$ErrorActionPreference = 'Stop'
$Repo = Split-Path -Parent $PSScriptRoot
$Tag = "v$Version"
$Exe = Join-Path $Repo 'target\release\QuickAchieve.exe'
$NotesRel = "docs/RELEASE-$Tag.md"
$Notes = Join-Path $Repo ($NotesRel -replace '/', '\')
$Identity = @('-c', 'user.name=QQQOrange',
              '-c', 'user.email=62794181+QQQOrange@users.noreply.github.com')

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
    $rt = cargo test 2>&1 | Select-String -Pattern '^test result:'
    if ($LASTEXITCODE -ne 0) { Fail 'cargo test 退出码非 0' }
    if (-not $rt -or ($rt -join ' ') -notmatch 'ok\.') { Fail 'cargo test 没有 "ok." 结果行' }
    Ok ('cargo test  ' + ($rt -join ' | '))

    if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
        $fb = 'C:\Users\64381\.dsh\dsh-runtimes\dsh-primary-runtime\dependencies\node\bin'
        if (Test-Path $fb) { $env:PATH = "$fb;$env:PATH" }
    }
    if (-not (Get-Command node -ErrorAction SilentlyContinue)) { Fail '找不到 node' }
    foreach ($suite in @('tools/test-ui.js', 'tools/test-pages.js')) {
        $out = node $suite 2>&1 | Select-String -Pattern '通过|FAIL'
        if ($LASTEXITCODE -ne 0) { Fail "$suite 退出码非 0" }
        if ("$out" -match 'FAIL') { Fail "$suite 有失败项" }
        if ("$out" -notmatch '全部通过') { Fail "$suite 没有打印 全部通过" }
        Ok ("$suite  " + ($out | Select-Object -Last 1))
    }

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
    $patched = [regex]::Replace($text, '(?m)^\*\*SHA256:\s*[0-9A-Fa-f]{0,64}\*\*\s*$', "**SHA256: $sha**")
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
    $rel = Invoke-RestMethod -Method Post -Uri 'https://api.github.com/repos/QQQOrange/QuickAchieve/releases' `
        -Headers $h -InFile $bodyFile -ContentType 'application/json; charset=utf-8'
    Ok ("Release id={0}" -f $rel.id)
    $asset = Invoke-RestMethod -Method Post `
        -Uri ("https://uploads.github.com/repos/QQQOrange/QuickAchieve/releases/{0}/assets?name=QuickAchieve.exe" -f $rel.id) `
        -Headers $h -InFile $Exe -ContentType 'application/octet-stream' -TimeoutSec 300
    Ok ("资产 {0}  {1:N0} B  {2}" -f $asset.name, $asset.size, $asset.state)

    Step '7/7 回读校验'
    $latest = Invoke-RestMethod -Uri 'https://api.github.com/repos/QQQOrange/QuickAchieve/releases/latest' `
        -Headers $h -TimeoutSec 20
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
