# Windows 安装包（无 Inno Setup 的替代链路）

`apps\windows\installer\build.ps1` 是正式发版链路：Inno Setup 编出可签名的 setup exe。
**但它要求编译机上装了 Inno Setup 7**。这台机器没装、也装不上（无网络），所以另有一条只依赖
Windows 自带组件的链路，用来出「发给别人的整包装机」：

```powershell
powershell -ExecutionPolicy Bypass -File tools\windows-package\build-package.ps1
```

产物在 `dist\Qingjian-<序号>-installer\Qingjian-<版本>-Windows-setup.exe`，单文件自解压，双击即装。

## 它做了什么

1. release 构建 Server、设置程序、TSF DLL（64 位与 32 位各一份）；
2. 把二进制与**随包产品数据**装进 `payload\`；
3. 生成 `install.cmd` / `uninstall.cmd` / `readme.txt`（模板在 `templates\`）；
4. 压成 `payload.zip`，用 IExpress 封成单文件 exe；
5. **审计**：每个进包的文件都要与仓库里某个构建产物 SHA256 一致，且不许有个人数据文件名。

## 个人数据不进包（这是这条链路最要紧的事）

个人词库、个人术语表、个人释义表、输入日志、学习数据**只在 `%APPDATA%\Qingjian`**，
安装包只装仓库里的产品数据（`data\generated`、`data\models`、`assets`）。
第 5 步的审计就是守这条线：任何不是仓库产物的文件都让脚本直接失败。

换句话说：**把这个包发给十个人，他们各自从零开始学自己的习惯，不会继承别人的词。**

## 本机踩到的坑

- **IExpress 在含空格的路径下静默失败**。仓库路径里有 `deepseek harness`，直接调它会返回 0 但不产 exe。
  脚本因此在 `%TEMP%\qingjian-pkg`（无空格）里构建再拷回来。
- **Windows PowerShell 5.1 按 ANSI 读 `.ps1`**。脚本里有中文注释时，无 BOM 的 UTF-8 文件会被解成乱码，
  字节错位把语法搞坏（报「Unexpected token '}'」）。**`.ps1` 必须存成带 BOM 的 UTF-8**；
  同理，交给 `cmd` 的 `.cmd` 脚本用 GBK 编码（模板写好中文，脚本按 GBK 落盘）。
- **`for` 语句不能放进圆括号当表达式**（5.1 不支持）：写 `$(for (...) { ... }) -join "\r\n"`。
- **提权探测别用 `net session`**：Server 服务没开时它会误报非管理员。改用 `reg query "HKU\S-1-5-19"`。
- **IExpress 压缩 160 MB 要 5 分钟**，别当成卡死。

## 安装脚本的几个要点

- 提权后的副本要 `-Wait`：IExpress 等脚本退出就删临时目录，副本还没拷完文件就没了。
- **TSF DLL 被每个应用进程加载着，覆盖不了**。脚本先把旧 DLL 改名腾位（运行中也能改名），
  再拷新的，最后删旧文件——与 Inno 安装器同一套办法（见 `installer\qingjian.iss` 文件头「升级」）。
- Server 通过 `explorer.exe` 启动，这样它落在登录用户的完整性级别上，不继承提权窗口。
- 没签名的 exe 不带 uiAccess（`QINGJIAN_UIACCESS=0`）：带 uiAccess 又没签名会起不来（os error 740）。
  代价是候选窗在任务栏搜索 / 系统设置里可能被盖住。

## 验证过的事

- 从解压出来的布局直接跑 Server：词库 132,884 条、笔画码表 92,184、释义表 44,924、
  英文词表 94,568、emoji 9,587、语言模型（bigram 4,913,987）全部加载成功，`learned=0`。
- 40 个进包文件逐个 SHA256 对上仓库产物；个人数据文件名一个没有。
