# 性能基线：把"启动多快、空闲多省、内存多大"变成可回归的数字
#
# 为什么要有它（v2.23.43）：
#   以前这些数字都是我手测一次、写进发布说明就算完 —— 改十版之后没人知道
#   到底变慢了多少。这个脚本每次跑都输出同一张表，并和 tools/perf-baseline.json
#   里的基线比对，超过阈值就非零退出（可以塞进发布脚本或 CI）。
#
# 用法：
#   pwsh -File tools/perf-baseline.ps1              # 测量 + 和基线比
#   pwsh -File tools/perf-baseline.ps1 -Update      # 把这次结果写成新基线
#
# 注意：用独立的数据目录和实例名，不碰你正在用的配置。

param(
    [switch]$Update,
    [int]$IdleSeconds = 12
)

$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$exe = Join-Path $root 'target\release\QuickAchieve.exe'
if (-not (Test-Path $exe)) { Write-Error "先构建：cargo build --release" }

$tmp = Join-Path $env:TEMP ("qs-perf-" + [guid]::NewGuid().ToString('N').Substring(0, 6))
New-Item -ItemType Directory -Path $tmp | Out-Null
$env:QS_DATA_DIR = $tmp
$env:QS_INSTANCE_TAG = 'perf' + (Get-Random)
$env:QS_DEBUG = '1'

$sw = [System.Diagnostics.Stopwatch]::StartNew()
$p = Start-Process -FilePath $exe -ArgumentList '--no-elevate' -PassThru
$log = Join-Path $tmp 'log.txt'

# 1) 启动 → 托盘就绪：以日志里 "hotkey: 独占" 或 "bar hwnd =" 行为准
$ready = $null
for ($i = 0; $i -lt 200; $i++) {
    Start-Sleep -Milliseconds 50
    if (Test-Path $log) {
        $t = Get-Content $log -Encoding UTF8 -Raw
        if ($t -match 'hotkey: 独占|bar hwnd =') { $ready = $sw.ElapsedMilliseconds; break }
    }
}
if (-not $ready) { $ready = -1 }

# 2) 空闲 CPU：等界面稳定，然后采样 IdleSeconds 秒
Start-Sleep -Seconds 3
$p.Refresh()
$cpu0 = $p.TotalProcessorTime.TotalMilliseconds
$t0 = Get-Date
Start-Sleep -Seconds $IdleSeconds
$p.Refresh()
$cpu1 = $p.TotalProcessorTime.TotalMilliseconds
$wall = ((Get-Date) - $t0).TotalMilliseconds
$idleCpuPct = if ($wall -gt 0) { [math]::Round(($cpu1 - $cpu0) / ($wall * [Environment]::ProcessorCount) * 100, 2) } else { -1 }

# 3) 内存与句柄（等 GC 稳定后再读）
$p.Refresh()
$ws = [math]::Round($p.WorkingSet64 / 1MB, 1)
$priv = [math]::Round($p.PrivateMemorySize64 / 1MB, 1)
$handles = $p.HandleCount
$threads = $p.Threads.Count

Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
Remove-Item Env:QS_DATA_DIR, Env:QS_INSTANCE_TAG, Env:QS_DEBUG -ErrorAction SilentlyContinue
Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue

$now = [ordered]@{
    startupToTrayMs = [int]$ready
    idleCpuPct      = [double]$idleCpuPct
    workingSetMB    = [double]$ws
    privateMB       = [double]$priv
    handles         = [int]$handles
    threads         = [int]$threads
    measuredAt      = (Get-Date -Format 'yyyy-MM-dd HH:mm')
    version         = (Get-Item $exe).VersionInfo.FileVersion
}

Write-Host ''
Write-Host '=== 本次测量 ==='
$now.GetEnumerator() | ForEach-Object { Write-Host ("  {0,-18} {1}" -f $_.Key, $_.Value) }

# 阈值：只对"明显退化"报警，正常波动（±30% / ±几百句柄）不算
$limits = @{
    startupToTrayMs = 1500         # 冷启动（含 WebView2 起来）实测 ~830ms，1.5s 是回归线
    idleCpuPct      = 1.5          # 空闲超过 1.5% 一个核就别扭了
    workingSetMB    = 90
    privateMB       = 45
    handles         = 900
}

$fail = @()
foreach ($k in $limits.Keys) {
    $v = $now[$k]
    if ($v -gt $limits[$k]) { $fail += ("{0} = {1}，超过上限 {2}" -f $k, $v, $limits[$k]) }
}

$baseFile = Join-Path $PSScriptRoot 'perf-baseline.json'
if ($Update) {
    ($now | ConvertTo-Json) | Set-Content -Path $baseFile -Encoding UTF8
    Write-Host ''
    Write-Host "基线已更新：$baseFile"
    exit 0
}
if (Test-Path $baseFile) {
    $base = Get-Content $baseFile -Encoding UTF8 -Raw | ConvertFrom-Json
    Write-Host ''
    Write-Host '=== 与基线对比 ==='
    foreach ($k in @('startupToTrayMs', 'idleCpuPct', 'workingSetMB', 'privateMB', 'handles')) {
        $b = $base.$k
        if ($null -ne $b -and $b -ne 0) {
            $delta = [math]::Round(($now[$k] - $b) / $b * 100, 1)
            Write-Host ("  {0,-18} {1,8} -> {2,8}  ({3}%)" -f $k, $b, $now[$k], $delta)
        }
    }
}

if ($fail.Count -gt 0) {
    Write-Host ''
    Write-Host '=== 超阈值 ==='
    $fail | ForEach-Object { Write-Host "  [FAIL] $_" }
    exit 1
}
Write-Host ''
Write-Host '[OK] 全部在阈值内'
exit 0
