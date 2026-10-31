# Play Integrity 配置规则与实现边界

核对日期：2026-10-31。上游引擎固定为 `23ec56c2440c4fd1a5e893a23ef7861d6366a828`。
本文记录可复核的规则依据、兼容选择及验证边界，不提供可保证通过的设备配置。

## 判定的依据

[Google 的公开判定说明](https://developer.android.com/google/play/integrity/verdicts#device-integrity-field)
区分设备、应用和账号信号。Android 13 及以上的 Device 要求硬件支持的锁定启动及认证系统证明；
Strong 还要求最近一年内的系统及 vendor 等分区安全更新。Android 12 及以下的 Strong 没有相同的补丁新鲜度要求。
Android 13 及以上的 Basic 要求证明的 root of trust 由 Google 提供；此版本范围内的可选 Basic／Strong 标签只向通过 Google Play 安装或更新的请求应用返回。未获许可时 Device 等满足条件的其他标签仍可能返回。Basic、Play 商店认证、本地证书链检查和服务端完整性结果不能互相替代。

修改本地 SDK、日期、指纹或证书显示，不能证明真实安全更新、锁定状态、设备认证或服务端接受。
本项目不将“Pixel 全被拉黑”“首发 API 32–35 必过”“36 必须 RKP”写为确定的服务端规则。
授权社区导出中的消息 26595–26610、26681、28620、28966、29043 提供相互冲突的经验与个案；
它们没有公开服务端实现、完整测试条件或可独立验证的完整性令牌，不能据此建立通用允许／拒绝名单。
导出的原文、人员信息、密钥与附件不随源码或安装包分发。

消息 26593、26596、26639、28962 描述了无需 PIF 或关闭 PIF 的经验路径。
因此 PIF 是独立可关闭的能力，不因导入 keybox、生成指纹或应用配置而自动重新启用。
关闭不接管或关闭 Tricky Store／TEESimulator／OhMyKeymint；这些功能仍由现有适配器管理。
不根据 Key Attestation 的显示或社区经验自动判断设备已满足 Google 三种完整性标签。

## 配置语义

| 能力 | 规则与处理 |
| --- | --- |
| Build 与系统属性 | 分别配置，分别生效；对显式不一致的对应字段给出预览提示，保持用户值。迁移只补缺失的对应属性，不覆盖现有值。 |
| Build 值类型 | 按实际反射字段处理 String、int、long 和 boolean；JSON 布尔规范化后的 1／0 与文本 true／false 均可用于布尔字段。非法类型或数值只跳过对应字段并记录日志，不把无法识别的布尔值静默转为 false。 |
| 文本编码 | 配置使用有效 UTF-8，原生 prop 解析也拒绝非法字节。Java 字符串按标准 UTF-8 显式解码，避免把补充平面字符交给只接受 Modified UTF-8 的 NewStringUTF。 |
| 首发 SDK 与 board API | 普通配置和迁移不注入 25／32；可以通过字段编辑器或原文设置，缺少时提示。保留已有值。首发 API 与当前系统 SDK 含义不同；不能用首发 SDK 直接推断服务端采用哪一组 Android 版本条件。 |
| Provider | 新配置及显式迁移缺省写入 `spoofProvider=0`，不覆盖已有开关。历史配置没有该字段时保留原生兼容默认 1；界面可显式选择。此 hook 阻止 DroidGuard 读取证书链，不代表生成或增强硬件证明。 |
| 签名 | `spoofSignature` 默认关闭；仅改变进程内包信息，不能改变服务端应用许可或签名认证。 |
| Play 商店 | 指纹开关 0 关闭、1 使用配置指纹，也可指定自定义指纹；SDK 为 0 时关闭、1 对应上游兼容值 32，其余正数为目标 SDK 上限。SDK 兼容优先于指纹，不声称改变真实系统版本。 |
| 属性通配符 | 只支持前导 `*` 的后缀匹配，精确键优先；其他形式提示不支持。仅 hook 已存在属性的 `__system_property_read_callback` 路径，不创建属性，不覆盖所有原生读取 API。 |
| Canary | 按既定兼容选择保留上游映射：Pixel 10a 使用 35（上游注释声称实际 36），未知设备使用 37，board API 使用 32。发布日期来自 HTTPS 响应头，六周失效及无法取到精确补丁时的回退日期均为估计。生成时保留一致的 Build／属性补丁值，不再依据 keystore 目录注释属性项；可在字段编辑器自行移除。预览明确说明，不据此判断证书有效期或服务端接受。 |
| 导入与迁移 | GPL CLI 将配置作为数据解析，不执行 `eval`；保留未知字段和源文件，输出覆盖前保留备份。生成只准备预览，不发布配置或修改 keystore 策略。 |
| 文件权限 | 引擎自有数据目录保持私有。独立迁移或下载写入已有外部目录时保留目录权限；仅将新建目录和输出文件设为私有，不收紧共享目录的访问权限。 |
| 安全补丁联动 | 引擎只建议日期；现有 keystore 适配器持有策略映射与写入。默认不勾选联动，应用经过预览、持久备份及后端事务记录；中断后阻止再次应用，由用户确认恢复。恢复按适配器 restart_keys 报告重启需求。修改策略不等于安装更新。 |

## Root 与生命周期

| 场景 | 行为与证据边界 |
| --- | --- |
| Magisk Zygisk | 安装目录不作为启用证据；使用 daemon 传入的 `ZYGISK_ENABLED=1` 或同次启动记录。外置 provider 只检查有效模块声明及 disable/remove 标记，不据此声称目标进程已注入。 |
| 无配置 | 完整模式须先应用有效非空配置，才执行全局兼容脚本；明确保存 scripts-only 后可独立运行。缺配置时旧敏感属性处理器保留所有权；boot hash 仍由原处理器持有。 |
| ROM 遮罩 | 仅允许受支持分区 app／priv-app／overlay 下单个条目；根目录、父目录、overlay 通配符和 config 目录均拒绝。CLI 保存与脚本运行前均校验，包括展开后的名称。 |
| DenyList | 保留上游自动管理：完整模式在强制名单开启时移除 GMS／Play 商店；scripts-only 模式添加。CLI 按包名和进程记录原值、预期值及未完成操作，写入前保存，写后读回。关闭仅恢复可核对的自有改动；外部编辑保留并提示。Magisk 在强制名单开启时添加条目可能终止相应进程，恢复操作也可能触发。未记录的历史改动不推测原值。 |
| 早期启动 | 在模块挂载之前准备 ROM 冲突遮罩及早期 OEM 属性。使用 `resetprop -n` 避免触发 init 属性服务；不联网、不等开机完成。PI 无配置、停用、冲突或失败的回退路径也只处理早期属性；锁定状态、AVB 默认值及保存的 boot hash 等待开机完成。 |
| 开机完成 | 入口与 PI 阶段适配层均要求 `sys.boot_completed=1`，未完成时拒绝执行晚期处理。延后处理 verified boot、锁定状态等 OEM 敏感属性，保持上游避免指纹识别／显示模式故障的时序。 |
| KernelSU late-load | 由 `late-load.sh` 代替早期阶段；当前 KernelSU 源码随后加载属性并调用 metamodule 挂载，再执行后续阶段。挂载实现与 OEM 行为须实机验证。 |
| SELinux | 保留 permissive 时限制 enforce/policy 文件读取权限的兼容行为；不是切换内核 enforcing。记录原始权限，关闭时仅恢复未被外部修改的权限；持久属性恢复记录对引号等字符作 shell 数据转义，按上游写回原值，可能覆盖后续对相同持久属性的修改；此行为在关闭界面明确告知。 |
| 独立关闭 | 先发布停用令牌，再取消生成任务和恢复有记录的兼容状态；非法配置、写锁忙或待恢复事务不阻止停用标记。显式启用只确认当前停用令牌，较早的启用请求不能覆盖后来的关闭。原生 companion、后台任务与 Shell 均检查此状态。已注入进程需重启，已有挂载及全局属性需重启设备。清理失败保留记录并提示重试，不重新启用。 |
| 停用令牌与通信 | 空停用令牌不能作为有效启用确认，Rust、原生 companion 与 Shell 保持一致。companion 帧头、DEX 与配置传输共享两秒单调时钟截止时间；使用非阻塞 socket，慢速分段数据不会重复延长窗口。接收失败清空输出，超时不作为注入证据。 |
| 发行架构 | 当前管理程序及引擎 CLI 为 arm64，注入库包括 arm64 与 ARMv7 进程版本；ARMv7 注入库不表示支持 32 位系统。安装器在准备运行数据之前拒绝不匹配的系统架构。 |
| 禁用／冲突 | 禁用或有活动 PIF／USNF 时跳过注入及引擎属性脚本；早期清理自有遮罩并恢复持久设置。现有属性处理器继续执行原路径及 boot hash 职责。全局修改及已运行进程的 hook 需要重启清除。 |

## 固定源码依据

- [Magisk a51dc10f：模块脚本与挂载顺序](https://github.com/topjohnwu/Magisk/blob/a51dc10f88ea3a8aac7fa02e95ae7589a034730b/native/src/core/module.rs)：先执行模块 post-fs-data，再处理 system.prop 与模块树。
- [Magisk a51dc10f：脚本环境](https://github.com/topjohnwu/Magisk/blob/a51dc10f88ea3a8aac7fa02e95ae7589a034730b/native/src/core/scripting.cpp)：Zygisk 启用信息由 daemon 显式传入。
- [Magisk a51dc10f：Zygisk API](https://github.com/topjohnwu/Magisk/blob/a51dc10f88ea3a8aac7fa02e95ae7589a034730b/native/src/core/zygisk/api.hpp)：强制卸载仅作用于进程挂载命名空间，不表示完整性通过。
- [Magisk a51dc10f：DenyList CLI](https://github.com/topjohnwu/Magisk/blob/a51dc10f88ea3a8aac7fa02e95ae7589a034730b/native/src/core/deny/cli.cpp)及[名单操作](https://github.com/topjohnwu/Magisk/blob/a51dc10f88ea3a8aac7fa02e95ae7589a034730b/native/src/core/deny/utils.cpp)：输出为 `package|process`，添加名单可触发进程终止。
- [KernelSU 3352209f：启动阶段](https://github.com/tiann/KernelSU/blob/3352209f44fc9c5f3ec6dff228080118433cb72c/userspace/ksud/src/init_event.rs)及 [late-load](https://github.com/tiann/KernelSU/blob/3352209f44fc9c5f3ec6dff228080118433cb72c/userspace/ksud/src/late_load.rs)。
- [AOSP frameworks/base 1cdfff55：Build](https://android.googlesource.com/platform/frameworks/base/+/1cdfff555f4a21f71ccc978290e2e212e2f8b168/core/java/android/os/Build.java)、[SystemProperties JNI](https://android.googlesource.com/platform/frameworks/base/+/1cdfff555f4a21f71ccc978290e2e212e2f8b168/core/jni/android_os_SystemProperties.cpp)及 [AndroidKeyStoreSpi](https://android.googlesource.com/platform/frameworks/base/+/1cdfff555f4a21f71ccc978290e2e212e2f8b168/keystore/java/android/security/keystore2/AndroidKeyStoreSpi.java)。
- [AOSP bionic 731631f3：属性读取](https://android.googlesource.com/platform/bionic/+/731631f300090436d7f5df80d50b6275c8c60a93/libc/system_properties/system_properties.cpp)：同步 callback、cookie 与只读属性长值语义；原生 Get 另走 Read。
- [AOSP system/core a3b721a3：属性服务](https://android.googlesource.com/platform/system/core/+/a3b721a32242006b59cb12bd62c9133632af3a2d/init/property_service.cpp)：ro 属性 write-once 及派生属性逻辑。
- [Android NDK：JNI 字符串说明](https://developer.android.com/ndk/guides/jni-tips#utf-8_and_utf-16_strings)：NewStringUTF 接受 Modified UTF-8；标准 UTF-8 输入需要显式解码。

## 属性与 keystore 配置边界

- Prop Setting 接受 AVB 的 SHA-256／SHA-512 十六进制摘要；OMK 的 `vb_hash` 固定为 32 字节，
  不兼容的摘要在写入共享开关、hash 文件或策略前拒绝。只有 hash 实际改变时才同步 backend policy，
  单独切换 handler 保留已有独立 `vb_hash`。已有 AVB 元数据不覆盖；缺失的算法默认值按摘要长度选择。
- 普通保存与自动加名单先读取及校验模块状态，再与后端配置一起发布；I/O 失败恢复受影响文件。普通保存保留未知的默认／逐应用策略键，旧页面同时校验后端身份和配置。
- Prop Setting 先读取全部受影响文件，后端与共享配置发布失败时恢复原内容；实时属性在配置文件成功发布后设置。
  清空 hash 只停止后续覆盖，已应用的属性需重启清除；handler 变更及 OMK 重启字段变更在保存结果中报告重启需求。
  文件恢复覆盖普通 I/O 失败，不构成跨文件掉电原子性保证。
- 普通保存、自动加名单、Prop Setting 和补丁事务共用 feature 内的进程写锁；释放锁时显式 unlock，
  不等待 fork 子进程的临时文件描述符副本关闭。该锁不协调第三方工具；编辑器提交读取时的配置作为乐观并发检查，
  防止旧页面覆盖本项目另一入口或第三方已经完成的策略变更。Prop Setting 同样提交读取时的共享开关与 hash，
  拒绝旧表单覆盖外部已保存的设置。保存返回适配器实际读回配置，保留各后端的规范化语义。
- 属性表单以实际载入的状态作为编辑基线，后台刷新不改变未保存编辑及其预期值；显式刷新重新载入已保存状态。策略表单也保留未保存编辑。
- PI 的补丁建议按启用的 Build／属性 hook 计算，属性优先级与原生 hook 一致：精确键先于前导 `*` 后缀规则。
  明确冲突、空 Build 值、全部关闭及无效日历日期均不建议自动联动；保留原配置，冲突通过预览提示。
  配置中的 `ro.vendor.build.security_patch`／`ro.vendor.boot_security_patch` 与系统补丁日期不同（包括匹配的通配符）时，跳过统一日期联动并提示，保留配置。
  keystore 的 per-app policy 仍优先于 default policy；默认策略联动不覆盖 per-app policy 或不识别的扩展键。
- CLI、Shell 与原生 companion 均遵守模块 disable/remove 标记；这些标记停止接管但不写入 PI 的独立停用令牌。原生 companion 也拒绝非法类型的兼容布尔选项，与 CLI 校验一致。
- SELinux 的属性文本和文件权限不等于内核 enforcing 状态；不会为修复检测提示而切换内核 SELinux 状态。

补充核对源码：

- [AVB 76117860：SHA-256／SHA-512 摘要](https://android.googlesource.com/platform/external/avb/+/761178607206f4cb2af79ed9eec52d8cbd814adb/libavb/avb_slot_verify.c)。
- [Android common 32521e28：SELinux enforce 读取与状态更新](https://android.googlesource.com/kernel/common/+/32521e28daaacb02caff14090988d91503e61978/security/selinux/selinuxfs.c)。
- [PIF 23ec56c：OEM 属性延后时序](https://github.com/osm0sis/PlayIntegrityFork/blob/23ec56c2440c4fd1a5e893a23ef7861d6366a828/module/service.sh)。
- [Tricky Addon cf167849：共享属性设置](https://github.com/KOWX712/Tricky-Addon-Update-Target-List/blob/cf167849aaa7696972a3c7826ec94294e9e47fce/webui/src/dialog/prop.ts)及 [prop.sh](https://github.com/KOWX712/Tricky-Addon-Update-Target-List/blob/cf167849aaa7696972a3c7826ec94294e9e47fce/module/prop.sh)。
- [OMK b12557c：32-byte trust 值及运行时重载](https://github.com/qwq233/OhMyKeymint/blob/b12557c1569ed15150acd7886c825de500fc9ee9/src/config.rs)，对应 [构建制品 12147111988](https://github.com/qwq233/OhMyKeymint/actions/runs/39155552658/artifacts/12147111988)。
- [OMK b12557c：分区补丁解析及系统属性同步](https://github.com/qwq233/OhMyKeymint/blob/b12557c1569ed15150acd7886c825de500fc9ee9/src/plat/vbmeta.rs)：system、vendor、boot 分别解析；非 auto 的 security_patch 在需要时写入全局系统属性，不表示实际系统更新。
- [WebUI X Portable ff81674c：KernelSU 兼容 spawn 回调](https://github.com/MMRLApp/WebUI-X-Portable/blob/ff81674ce61b1d7a4cfcdb29736ec022968fc651/app/src/main/java/com/dergoogler/mmrl/wx/ui/webui/interfaces/ksu/KernelSUInterface.kt)：先发送输出，再报告 exit，非零退出随后报告 error；共用桥接保持该协议，不因失败重试写操作。

## 可复现验证与限制

源码组件以普通目录嵌入主仓库，构建入口为 `scripts/build-play-integrity.py`。
根 Zygisk 库统一组合 PI 与 Device Faker，安装包只有一个 `module.prop`；
构建拒绝缺少组合运行时的独立 native 入口，打包拒绝组件被替换为 Git gitlink。
GPL 与依赖许可证、原始版权、NOTICE 及固定来源版本均保留；源码入口指向构建提交。
CLI 的预览、配置、生成、迁移、备份恢复、关闭和诊断能力仍通过进程契约提供。

Linux／Android common 的 [bootconfig 初始化](https://android.googlesource.com/kernel/common/+/e99406ef2429696d82cc58f2d5c24721c5a2e133/init/main.c)
与[内核文档](https://docs.kernel.org/admin-guide/bootconfig.html)区分内核启动配置和用户空间属性。
修改 `ro.boot.*` 不会重写内核 `/proc/bootconfig`、命令行或硬件 root of trust。
AOSP `PropertyLoadBootDefaults` 按分区优先级加载后再派生产品属性及指纹；
`Build` 静态字段与 bionic 属性 callback 是不同的读路径，不能把属性文本变更视为已加载 Java 字段同步更新。
晚期阶段只检查启动完成，不在阻塞的早期阶段等待。阶段进程自身的超时不延长 root manager 的预算：
[KernelSU](https://kernelsu.org/guide/module.html#boot-scripts) 的 post-fs-data 上限为 10 秒；
[Magisk](https://topjohnwu.github.io/Magisk/guides.html#boot-scripts) 还有整体挂载预算。
组合管理程序在开机完成后的 PI 调用使用 35 秒外层预算，覆盖 PI 的 30 秒脚本预算及有界后处理；
不会因通用命令的 5 秒预算提前杀死仍正常执行的晚期阶段。原属性处理器、原生 companion 等通用命令仍保持 5 秒预算。
PI 子进程通过 [waitid 的 WNOWAIT 语义](https://man7.org/linux/man-pages/man2/waitpid.2.html)
先保留可回收子进程身份、终止所属进程组，再回收状态；ECHILD 后不再按数字 PID 发信号。
依据 [Android common 等待实现](https://android.googlesource.com/kernel/common/+/e99406ef2429696d82cc58f2d5c24721c5a2e133/kernel/exit.c)
及 [bionic wait 接口](https://android.googlesource.com/platform/bionic/+/731631f300090436d7f5df80d50b6275c8c60a93/libc/include/sys/wait.h)。

[Zygisk Next 文档 cddf38d5](https://github.com/Dr-TSNG/ZygiskNext/tree/cddf38d5b1501d0cbf08393767afed09ac478b14)
要求 Magisk 内置 Zygisk 与外置 provider 不同时启用。本组件使用保留 ISC 声明的标准 v4 ABI，
没有复制或打包 ZN 的保留权利实现。其 HyperOS Rust Runtime 文档明确不提供 ART／JNI；
当前 Java 注入路径不据此声明支持该运行时。provider 安装目录仅是前提信息，不能证明某个目标进程已注入。

Pixel 社区属性库只按用户请求联网，目录与文件固定到同一 Git 提交。
UTC 生成时间、设备、渠道、文件路径、构建、增量版本、精确补丁日期及摘要均先校验；
不将重复属性按最后一项静默覆盖。产品分区身份用于候选，通用 system 指纹不覆盖真实机型身份。
首发 API 不作为当前 SDK 或 board API 的推断依据。system、vendor、boot 补丁分别保留；
SHA-256／SHA-512 vbmeta 摘要仅作来源参考，不随 PI 候选写入全局属性或 keystore 策略。
来源快照核对完整指纹、型号、首发 API 与对应属性，避免修改后的配置仍标记为原始构建。
官方 Pixel 新来源记录通过 `identity_bound=true` 同时绑定完整发布身份、增量版本、型号与首发 API；
保存及读取快照都核对 Build 字段及已存在的对应属性。旧记录仍兼容读取，但明确提示只记录设备与构建；
来源数据本身不是签名或服务端判定证明。自定义身份仍可保存为不带来源的普通快照。
官方及社区 Pixel 候选都在同一配置写锁下读取高级选项并生成预览基线；
锁忙或未遵守锁的外部写入改变版本时拒绝候选，避免把旧选项配上新版本后覆盖更新。

页面沿用语义色、统一圆角和间距，参考 [M3 Expressive 的视觉层级](https://design.google/library/expressive-material-design-google-research)、
[Material 无障碍触控尺寸](https://developer.android.com/develop/ui/compose/accessibility/api-defaults#minimum-touch-target-sizes)
以及 [Apple 布局](https://developer.apple.com/design/human-interface-guidelines/layout)的对齐、分组和渐进披露原则。
属性库触控目标至少 48 CSS px，来源与摘要可展开；表格在局部滚动区域保留可读列宽。
渠道、错误标题与日期按界面语言显示，技术字符串保持方向隔离。
候选及补丁预览只在准备完整后发布；关闭或更新选择使旧请求失效。
较早生成任务完成后不覆盖后来选中的候选，结果保留供用户重新预览。
轮询和取消响应还须匹配当前存活页面及任务 ID；过期响应不清空新任务标识，
不发布旧候选，也不显示已经失效的错误提示。获取目录与准备配置分别报告状态。
编辑器、文件导入和快照预览在发送请求前确定候选代次；内容同步修改、来源切换、关闭预览或离开页面后，
旧响应不能重新发布候选。当前请求的网络或补丁准备错误仍正常报告。
生成启动请求的存储适配层只存活到请求结束，页面关闭后仍保存返回的任务 ID；不会覆盖后来已跟踪的其他任务。
重新进入页面或任务 ID 跨视图更新时继续轮询，页面本身的轮询与 UI 更新在离开时释放。
Canary 日期按界面语言显示，以 UTC 的日期语义格式化，避免负时区把发布日期显示成前一天；未知文本仍保留原值。
打包时逐字节核对主项目、PI、Device Faker 及随源码保留的依赖声明，
缺失或被截断的声明不能仅凭文件名存在而通过校验。

独立 Rust 测试覆盖未知字段、API 值保留、Provider 默认值、只读规则诊断、shell 元字符迁移、Zygisk 误判、超时与取消。
生成进程独立持有子孙进程组；取消、超时、输出超限、异常及正常完成后均终止残留进程，候选配置不会自动发布。
配置锁在所属对象结束时显式释放，不因 fork／exec 期间暂存的文件描述符副本延长锁期。
`python3 scripts/test-play-integrity-scripts.py` 使用隔离命令替身验证持久属性恢复、停用标记及 OEM 前后阶段；DenyList 模式与条件恢复由独立 Rust CLI 测试覆盖。
`python3 scripts/test-play-integrity-native.py` 检查原生配置、慢速与阻塞 companion 边界，并通过启用 CheckJNI 的主机 JVM 验证实际 UTF-8 字符串转换。
`python3 scripts/test-play-integrity-java.py` 在主机 JVM 验证实际 Build 值转换，包括规范化布尔值、数值边界和非法值。
WebUI 的真实后端契约与浏览器流程覆盖预览、字段编辑、导入、应用失败恢复与多语言。

`python3 scripts/test-integrity-conflicts-device.py --serial emulator-5554` 在一次性 rooted AVD 上验证已安装模块。
验收环境为 Android API 36 arm64、内核 6.6.66、KernelSU 3.3.0 late-load，以及上述 OMK 构建制品。
覆盖实时属性读回、hash／策略同步、独立策略保留、补丁事务、旧页面拒绝覆盖、非法输入无副作用及独立停用；
检查 OMK 进程仍存在且无配置解析错误，退出时恢复所改配置文件。脚本只接受 emulator serial；已应用实时属性需重启清除。
`DUCK_KSU_TOKEN=... pnpm --dir ui test:device --grep 'native WebUI bridge'` 使用已配置调试和 token 的 KernelSU Manager，
验证真实 WebUI 的 hash 输入、保存、实时属性与 OMK 策略读回、清空及重启提示；测试结束恢复所改设置。

这些检查不验证 Google 服务端判定。AVD 不证明物理设备上的 Zygisk 注入、完整属性 hook 覆盖、OEM 指纹／显示行为、
硬件 TEE、SELinux 权限影响或其他 root／挂载实现兼容；实时 Canary 元数据及上述硬件行为仍需真机验收。

## 增量配置与诊断契约

- 正式版 Pixel 获取默认选择 Pixel 6a，按设备及具体构建生成候选。Google 官方元数据来源不代表服务端认可；不维护“唯一可用机型”名单。精确 SPL 缺失时留空，禁止从构建编号或发布日期猜测补丁日期。
- 设备目录覆盖初代至 Pixel 11 系列的 39 款手机、折叠机与 Pixel Tablet，支持型号及代号搜索。
  代号对照 [AOSP 设备表](https://source.android.com/docs/setup/reference/build-numbers#device-code-names)
  与 [Google 设备目录](https://docs.cloud.google.com/developer-device-platform/device-catalog)；
  首发 Android 版本对照 [Google 当前机型规格](https://support.google.com/pixelphone/answer/7158570)、
  [早期机型规格](https://support.google.com/pixelphone/answer/16043605)及 [API 对照表](https://source.android.com/docs/setup/reference/build-numbers#platform-code-names-versions-api-levels-and-ndk-releases)。
  Pixel 10a 的真实首发 API 为 36，Pixel 11 系列为 37；此处不套用 Canary 的兼容映射。
  Flash Tool 返回的现存最早构建和云设备当前 SDK 均不能直接当作首发 API：例如现存 Pixel 6a 元数据可能从 Android 16 开始，首发 API 仍为 32。
  只在用户请求时获取对应设备的可用发布元数据，保留最多 40 个可解析构建；目录列出机型不保证 Google 始终提供构建。
  构建选择同时核对构建号及增量版本；旧调用省略增量版本时仅接受唯一匹配。切换机型、构建或重新获取时立即清除旧预览。
- 三项快捷配置只修改草稿开关。Play 商店仅替换 FINGERPRINT，SDK 兼容模式优先于商店指纹替换；两者同时设置时预览给出提示。
- 补丁参与者可接受 `partitions`，键为 system、vendor、boot，值为各自的 YYYY-MM-DD。只映射选中分区；省略时保留原有统一日期契约。所有格式、备份、写入与恢复仍由 keystore 适配器管理。
- 原生观测是进程初始化时的一次读回，不持续监控。管理端校验配置 SHA-256、启动标识、存活 PID、进程名及启动时间；日志不可用时保持未知，不将保存配置或 hook 安装成功等同于实际判定。
- 导入报告只保存白名单摘要。缺失标签记为未返回，不能在没有证据时归因于指纹、补丁或许可。报告真实性未经验证；参考 Google 的可选标签与许可要求。
