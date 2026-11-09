青简输入法 · Windows 安装包（版本 __VERSION__）
===============================================

这是什么
--------
青简是一个 Windows 中文输入法（TSF 文本服务）：全拼 / 双拼 / 注音 / 五笔与混输，
带英文模式、emoji、符号面板、主题、候选窗释义，以及本地整句模型（全程离线，不联网）。

包里有什么
----------
  qingjian-server.exe            输入法内核进程（自绘候选窗与状态条）
  qingjian-settings.exe          设置程序
  qingjian_tsf-__VERSION__.dll         TSF 输入法，64 位
  qingjian_tsf-__VERSION__-x86.dll     TSF 输入法，32 位（企业微信 / WPS / 32 位 QQ 这类应用要它）
  uninstall.cmd                  卸载
  readme.txt                     本文件
  data\generated\                词库、语言模型、各语言释义表、11 个领域词库、笔画码表
  data\models\                   含章·通变 / 含章·知微 本地整句模型
  assets\                        emoji、词汇等级表、五笔码表、样例词库

不含个人数据
------------
这个包不含任何个人词库、个人术语表和输入日志。那三样只存在实际使用者的
%APPDATA%\Qingjian 里，安装包从不携带——装上之后，输入法是从零开始学使用者自己的
习惯，不会继承别人的词。

自己的词库与术语想带走：拷 %APPDATA%\Qingjian 下的 user-words.tsv、user-terms.tsv、
user-glossary-*.tsv，放到新机器同一位置即可。

怎么装
------
1. 双击安装包，弹出 UAC 时选「是」（注册输入法需要管理员权限）；
2. 装完注销并重新登录（或重启电脑）——输入法 DLL 会装进每个应用进程，
   装之前就开着的应用必须重开才会用上新输入法；
3. 用 Win+空格或语言栏切到「青简」。

怎么用
------
  全拼      zhongguo  ->  中国
  五笔      设置 → 输入方案 → 五笔
  混输      拼音和五笔一起打，编码打全的形码候选排前面
  中英切换  Shift
  设置      开始菜单 → 青简 → 青简设置

卸载
----
开始菜单 → 青简 → 卸载青简，或运行安装目录下的 uninstall.cmd。
卸载会注销输入法并删除程序文件；个人数据默认保留，脚本会问要不要一起删。

已知限制
--------
- 这个包没有代码签名，Windows SmartScreen 可能提示「未知发布者」，选「仍要运行」。
- 没签名就用不了 uiAccess，候选窗在「任务栏搜索 / 系统设置」这类高权限界面上可能被盖住；
  浏览器、Office、聊天工具等普通应用不受影响。
- 首次启动会加载本地整句模型（约 100 MB），头几秒打字会稍慢，之后正常。

许可
----
青简本体：GPL-3.0-or-later。
随包第三方数据的许可在 data\generated\dicts\LICENSE-THUOCL.txt、
data\generated\codes\LICENSE-CNS11643.txt、assets\wubi\LICENSE、
assets\emoji\LICENSE-unicode.txt。
