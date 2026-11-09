<#
.SYNOPSIS
    在没有 Inno Setup 的机器上打青简 Windows 安装包：单文件自解压 exe（IExpress 容器 + install.cmd）。
.DESCRIPTION
    正式发布走 `apps\windows\installer\build.ps1`（Inno Setup，产出签名的 setup exe）。
    本脚本是它的无依赖替代：只用 Windows 自带的 IExpress 与 tar.exe，外加 7-Zip（可选，用于压 payload.zip）。

    步骤：
      1) release 构建 Server / 设置程序 / TSF DLL（64 位与 32 位各一份）；
      2) 把二进制与随包数据装进 payload 目录（**只装仓库产物，绝不装用户目录里的任何东西**）；
      3) 生成 install.cmd / uninstall.cmd / readme.txt；
      4) 压成 payload.zip，再用 IExpress 封成单文件 exe。

    **个人数据不会进包**：个人词库（user-words.tsv）、个人术语表（user-terms.tsv）、
    个人释义表（user-glossary-*.tsv）、输入日志、学习数据一律在 `%APPDATA%\Qingjian`，
    安装包只装仓库里的产品数据（data\generated、data\models、assets）。
    脚本最后跑一次 SHA256 审计：每个进包的文件都必须与仓库里的某个构建产物字节一致。

    .PARAMETER SkipBuild
        跳过 cargo build（产物已经是最新的时用）。
    .PARAMETER OutDir
        输出目录，缺省 `dist\Qingjian-<日期>-<序号>-installer`。
    .PARAMETER NoModels
        不带本地整句模型（省约 97 MB，整句重排随之关闭）。
.PARAMETER Version
    写进文件名与 readme 的版本号，缺省从 `apps\windows\server\Cargo.toml` 读。
#>
[CmdletBinding()]
param(
    [switch]$SkipBuild,
    [switch]$NoModels,
    [string]$OutDir = '',
    [string]$Version = ''
)

$ErrorActionPreference = 'Stop'

$Repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$SevenZip = 'C:\Program Files (x86)\7-Zip\7z.exe'

# IExpress 在含空格的路径下会静默失败（仓库路径就有空格），所以在临时目录里构建再拷回来。
$Work = Join-Path $env:TEMP 'qingjian-pkg'

# ---------------------------------------------------------------- 0) 版本号
if (-not $Version) {
    $cargoToml = Get-Content (Join-Path $Repo 'apps\windows\server\Cargo.toml')
    $verLine = $cargoToml | Where-Object { $_ -match '^\s*version\s*=\s*"(.+)"' } | Select-Object -First 1
    if (-not ($verLine -match '"(.+)"')) { throw '在 server\Cargo.toml 里没找到 version' }
    $Version = $Matches[1]
}
$Version = $Version -replace '-dev$', ''

# ---------------------------------------------------------------- 1) 构建
if (-not $SkipBuild) {
    Write-Host '构建 release 产物…' -ForegroundColor Cyan
    Push-Location $Repo
    try {
        # uiAccess 缺省不带：没签名的 exe 带 uiAccess=true 起不来（os error 740）。
        $env:QINGJIAN_UIACCESS = '0'
        cargo build --release --locked -p qingjian-windows-server -p qingjian-settings -p qingjian-windows-tsf
        if ($LASTEXITCODE -ne 0) { throw "cargo build 失败（退出码 $LASTEXITCODE）" }
        cargo build --release --locked -p qingjian-windows-tsf --target i686-pc-windows-msvc
        if ($LASTEXITCODE -ne 0) { throw "32 位 DLL cargo build 失败（退出码 $LASTEXITCODE）" }
    } finally { Pop-Location }
}

# ---------------------------------------------------------------- 2) 输出目录
if (-not $OutDir) {
    $seq = 1
    while (Test-Path (Join-Path $Repo "dist\Qingjian-$seq-installer")) { $seq++ }
    $OutDir = Join-Path $Repo "dist\Qingjian-$seq-installer"
}
$Payload = Join-Path $OutDir 'payload'
Remove-Item $OutDir -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Path $Payload -Force | Out-Null

function Copy-Payload($source, $relative) {
    $dest = Join-Path $Payload $relative
    New-Item -ItemType Directory -Path (Split-Path $dest -Parent) -Force | Out-Null
    Copy-Item (Join-Path $Repo $source) $dest -Force
}

# ---- 二进制 ----
Copy-Item (Join-Path $Repo 'target\release\qingjian-server.exe')   (Join-Path $Payload 'qingjian-server.exe') -Force
Copy-Item (Join-Path $Repo 'target\release\qingjian-settings.exe') (Join-Path $Payload 'qingjian-settings.exe') -Force
Copy-Item (Join-Path $Repo 'target\release\qingjian_tsf.dll')      (Join-Path $Payload "qingjian_tsf-$Version.dll") -Force
Copy-Item (Join-Path $Repo 'target\i686-pc-windows-msvc\release\qingjian_tsf.dll') (Join-Path $Payload "qingjian_tsf-$Version-x86.dll") -Force
Copy-Item (Join-Path $Repo 'apps\windows\tsf\resources\qingjian.ico') (Join-Path $Payload 'qingjian.ico') -Force

# ---- 随包数据（产品数据，不是个人数据）----
foreach ($file in @(
    'dict.qj', 'lm.qj', 'english.tsv',
    'glossary-en.qj', 'glossary-ja.qj', 'glossary-zh.qj', 'glossary-es.qj',
    'glossary-vi.qj', 'glossary-de.qj', 'glossary-ko.qj', 'glossary-fr.qj'
)) {
    $src = Join-Path $Repo "data\generated\$file"
    if (Test-Path $src) { Copy-Payload "data\generated\$file" "data\generated\$file" }
    else { Write-Warning "缺 $file，跳过" }
}
Get-ChildItem (Join-Path $Repo 'data\generated\dicts') -Filter *.qj -ErrorAction SilentlyContinue |
    ForEach-Object { Copy-Payload "data\generated\dicts\$($_.Name)" "data\generated\dicts\$($_.Name)" }
Copy-Payload 'data\generated\codes\stroke.qj' 'data\generated\codes\stroke.qj'
Copy-Payload 'assets\stroke\LICENSE-CNS11643.txt' 'data\generated\codes\LICENSE-CNS11643.txt'

# ---- 本地整句模型 ----
if (-not $NoModels) {
    foreach ($model in @(
        'hanzhang-tongbian\hanzhang-tongbian-small.qjm',
        'hanzhang-zhiwei\hanzhang-zhiwei-small.qjm'
    )) {
        $src = Join-Path $Repo "data\models\$model"
        if (Test-Path $src) { Copy-Payload "data\models\$model" "data\models\$model" }
        else { Write-Warning "缺模型 $model，整句重排会退到没有模型的样子" }
    }
}

# ---- 资源与许可声明 ----
Copy-Payload 'assets\emoji\emoji-zh.tsv'      'assets\emoji\emoji-zh.tsv'
Copy-Payload 'assets\emoji\emoji-en.tsv'      'assets\emoji\emoji-en.tsv'
Copy-Payload 'assets\emoji\LICENSE-unicode.txt' 'assets\emoji\LICENSE-unicode.txt'
Copy-Payload 'assets\levels\levels-en.tsv'    'assets\levels\levels-en.tsv'
Copy-Payload 'assets\levels\levels-ja.tsv'    'assets\levels\levels-ja.tsv'
Copy-Payload 'assets\wubi\wubi86.tsv'         'assets\wubi\wubi86.tsv'
Copy-Payload 'assets\wubi\LICENSE'            'assets\wubi\LICENSE'
Copy-Payload 'assets\sample\dict.tsv'         'assets\sample\dict.tsv'
Copy-Payload 'assets\lexicon\00_meta\THUOCL_LICENSE.txt' 'data\generated\dicts\LICENSE-THUOCL.txt'

# ---------------------------------------------------------------- 3) 安装 / 卸载脚本
$templates = Join-Path $PSScriptRoot 'templates'
foreach ($name in @('install.cmd.tpl', 'uninstall.cmd.tpl', 'readme.txt')) {
    if (-not (Test-Path (Join-Path $templates $name))) { throw "缺模板 $name" }
}

$shortcutPs = @(
    '$app = Join-Path $env:ProgramFiles "Qingjian"',
    '$ws = New-Object -ComObject WScript.Shell',
    '$startup = Join-Path $env:ProgramData "Microsoft\Windows\Start Menu\Programs\StartUp"',
    '$s = $ws.CreateShortcut((Join-Path $startup ([char]0x9752 + [char]0x7B80 + " Server.lnk")))',
    '$s.TargetPath = Join-Path $app "qingjian-server.exe"',
    '$s.WorkingDirectory = $app',
    '$s.IconLocation = (Join-Path $app "qingjian.ico") + ",0"',
    '$s.Save()',
    '$menu = Join-Path $env:ProgramData ("Microsoft\Windows\Start Menu\Programs\" + [char]0x9752 + [char]0x7B80)',
    'New-Item -ItemType Directory -Path $menu -Force | Out-Null',
    'foreach ($item in @(@(([char]0x9752 + [char]0x7B80 + [char]0x8BBE + [char]0x7F6E), "qingjian-settings.exe"), @(([char]0x5378 + [char]0x8F7D + [char]0x9752 + [char]0x7B80), "uninstall.cmd"))) {',
    '  $l = $ws.CreateShortcut((Join-Path $menu ($item[0] + ".lnk")))',
    '  $l.TargetPath = Join-Path $app $item[1]',
    '  $l.WorkingDirectory = $app',
    '  $l.IconLocation = (Join-Path $app "qingjian.ico") + ",0"',
    '  $l.Save()',
    '}'
) -join "`r`n"
$shortcutB64 = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($shortcutPs))

$removePs = @(
    '$startup = Join-Path $env:ProgramData "Microsoft\Windows\Start Menu\Programs\StartUp"',
    'Remove-Item (Join-Path $startup ([char]0x9752 + [char]0x7B80 + " Server.lnk")) -Force -ErrorAction SilentlyContinue',
    'Remove-Item (Join-Path $env:ProgramData ("Microsoft\Windows\Start Menu\Programs\" + [char]0x9752 + [char]0x7B80)) -Recurse -Force -ErrorAction SilentlyContinue'
) -join "`r`n"
$removeB64 = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($removePs))

$gbk = [Text.Encoding]::GetEncoding(936)
$install = [IO.File]::ReadAllText((Join-Path $templates 'install.cmd.tpl'), [Text.Encoding]::UTF8)
$install = $install.Replace('__SHORTCUT_B64__', $shortcutB64).Replace('__VERSION__', $Version)
[IO.File]::WriteAllText((Join-Path $OutDir 'install.cmd'), $install, $gbk)

$uninstall = [IO.File]::ReadAllText((Join-Path $templates 'uninstall.cmd.tpl'), [Text.Encoding]::UTF8)
$uninstall = $uninstall.Replace('__REMOVE_B64__', $removeB64).Replace('__VERSION__', $Version)
[IO.File]::WriteAllText((Join-Path $Payload 'uninstall.cmd'), $uninstall, $gbk)

$readme = [IO.File]::ReadAllText((Join-Path $templates 'readme.txt'), [Text.Encoding]::UTF8)
$readme = $readme.Replace('__VERSION__', $Version)
[IO.File]::WriteAllText((Join-Path $Payload 'readme.txt'), $readme, (New-Object Text.UTF8Encoding($true)))

# ---------------------------------------------------------------- 4) 压载荷
$zip = Join-Path $OutDir 'payload.zip'
if (Test-Path $zip) { Remove-Item $zip -Force }
if (Test-Path $SevenZip) {
    Push-Location $Payload
    try {
        # -mx=1：.qj 本来就压不动，外层 IExpress 还会再压一次
        & $SevenZip a -tzip -mx=1 $zip '.\*' | Out-Null
    } finally { Pop-Location }
} else {
    Write-Host '没找到 7-Zip，改用 Compress-Archive…' -ForegroundColor Yellow
    Compress-Archive -Path (Join-Path $Payload '*') -DestinationPath $zip -Force
}
if (-not (Test-Path $zip)) { throw 'payload.zip 没生成' }

# ---------------------------------------------------------------- 5) IExpress 容器
Remove-Item $Work -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Path $Work -Force | Out-Null
Copy-Item (Join-Path $OutDir 'install.cmd') $Work -Force
Copy-Item $zip $Work -Force
$exeName = "Qingjian-$Version-Windows-setup.exe"
$workExe = Join-Path $Work $exeName

$sed = @"
[Version]
Class=IEXPRESS
SEDVersion=3

[Options]
PackagePurpose=InstallApp
ShowInstallProgramWindow=0
HideExtractAnimation=1
UseLongFileName=1
InsideCompressed=0
CAB_FixedSize=0
CAB_ResvCodeSigning=0
RebootMode=N
InstallPrompt=
DisplayLicense=
FinishMessage=
TargetName=$workExe
FriendlyName=Qingjian
AppLaunched=install.cmd
PostInstallCmd=<None>
AdminQuietInstCmd=
UserQuietInstCmd=
SourceFiles=SourceFiles

[Strings]
FILE0="install.cmd"
FILE1="payload.zip"

[SourceFiles]
SourceFiles0=$Work\

[SourceFiles0]
%FILE0%=
%FILE1%=
"@
$sedPath = Join-Path $Work 'package.sed'
Set-Content $sedPath $sed -Encoding ascii

Write-Host '封成单文件 exe（IExpress/LZX，要几分钟）…' -ForegroundColor Cyan
& "$env:SystemRoot\System32\iexpress.exe" /N /Q $sedPath | Out-Null
Start-Sleep -Seconds 3
if (-not (Test-Path $workExe)) { throw 'IExpress 没产出 exe' }
Copy-Item $workExe (Join-Path $OutDir $exeName) -Force

# ---------------------------------------------------------------- 6) 审计：包里不许有个人数据
$staged = Get-ChildItem $Payload -Recurse -File | Where-Object { $_.Name -notin @('readme.txt', 'uninstall.cmd') }
$index = @{}
foreach ($root in @(
    (Join-Path $Repo 'data\generated'), (Join-Path $Repo 'data\models'), (Join-Path $Repo 'assets'),
    (Join-Path $Repo 'target\release'), (Join-Path $Repo 'target\i686-pc-windows-msvc\release'),
    (Join-Path $Repo 'apps\windows\tsf\resources')
)) {
    if (-not (Test-Path $root)) { continue }
    foreach ($file in Get-ChildItem $root -Recurse -File) {
        $hash = (Get-FileHash $file.FullName -Algorithm SHA256).Hash
        if (-not $index.ContainsKey($hash)) { $index[$hash] = $file.FullName }
    }
}
$unmatched = @()
foreach ($file in $staged) {
    $hash = (Get-FileHash $file.FullName -Algorithm SHA256).Hash
    if (-not $index.ContainsKey($hash)) { $unmatched += $file.FullName }
}
if ($unmatched) {
    Write-Host "审计失败：这些文件不是仓库产物：" -ForegroundColor Red
    $unmatched | ForEach-Object { Write-Host "  !! $_" -ForegroundColor Red }
    throw '包里有来源不明的文件'
}

$forbidden = @(
    'user-terms.tsv', 'user-words.tsv', 'user.tsv', 'user-ngram.tsv', 'user-vocab.tsv',
    'user-choices.tsv', 'user-typos.tsv', 'user-english.tsv', 'user-blocked.tsv',
    'user-glossary-en.tsv', 'user-glossary-ja.tsv', 'input-log.jsonl', 'config.toml',
    'update.json', 'usage.tsv', 'usage-hourly.tsv', 'sogou-user.tsv', 'user-terms-pending.tsv'
)
$leak = Get-ChildItem $Payload -Recurse -File | Where-Object { $forbidden -contains $_.Name }
if ($leak) {
    $leak | ForEach-Object { Write-Host "  泄漏：$($_.FullName)" -ForegroundColor Red }
    throw '包里有个人数据文件'
}

$size = (Get-ChildItem $Payload -Recurse -File | Measure-Object Length -Sum).Sum
Write-Host ''
Write-Host "审计通过：$($staged.Count) 个进包文件全部与仓库构建产物字节一致，没有个人数据。" -ForegroundColor Green
Write-Host "安装包：$(Join-Path $OutDir $exeName)" -ForegroundColor Green
Write-Host ("体积：{0:N1} MB（未压缩负载 {1:N1} MB）" -f ((Get-Item (Join-Path $OutDir $exeName)).Length / 1MB), ($size / 1MB)) -ForegroundColor Green
