//! 开机自启：往系统里登记一条启动项，让本程序随系统启动。
//!
//! 与完整版共用同一套历史数据，但自启项名必须区分开：注册表 HKCU\Run 与
//! macOS 登录项都按 `app_name` 取值，轻量版若沿用 "CC Switch" 会覆盖完整版
//! 的启动项，卸载其中一个就把另一个的自启一起抹掉。
//!
//! 轻量版额外支持「静默启动」：开机启动时带上 `--silent`，程序起来后只驻留
//! 托盘不弹主窗口。手动双击启动不带这个参数，所以照常显示窗口，不会被静默
//! 设置困住（否则用户再也看不到界面，只能去关自启）。

use crate::error::AppError;
use auto_launch::{AutoLaunch, AutoLaunchBuilder};

/// 启动项名字。刻意与完整版的 "CC Switch" 区分，两边各自独立启停。
const APP_NAME: &str = "CC Switch Lite";

/// 开机自启时附加的参数：只驻留托盘，不显示主窗口。
pub const SILENT_ARG: &str = "--silent";

/// macOS 上 AppleScript 登录项需要 `.app` 包路径：指向包内可执行文件会开一个终端窗口。
#[cfg(target_os = "macos")]
fn macos_app_bundle_path(exe_path: &std::path::Path) -> Option<std::path::PathBuf> {
    let path_str = exe_path.to_string_lossy();
    let app_pos = path_str.find(".app/Contents/MacOS/")?;
    Some(std::path::PathBuf::from(&path_str[..app_pos + 4]))
}

/// 构造自启项。`silent` 只影响 `enable` 写进启动项的命令行；`disable` / `is_enabled`
/// 只看同名键，参数无关，传 false 即可。
fn get_auto_launch(silent: bool) -> Result<AutoLaunch, AppError> {
    let exe_path =
        std::env::current_exe().map_err(|e| AppError::Message(format!("无法获取应用路径: {e}")))?;

    #[cfg(target_os = "macos")]
    let app_path = macos_app_bundle_path(&exe_path).unwrap_or(exe_path);

    #[cfg(not(target_os = "macos"))]
    let app_path = exe_path;

    // 空数组要显式标类型：set_args 收 `&[impl AsRef<str>]`，空切片推不出元素类型。
    let args: &[&str] = if silent { &[SILENT_ARG] } else { &[] };

    AutoLaunchBuilder::new()
        .set_app_name(APP_NAME)
        .set_app_path(&app_path.to_string_lossy())
        .set_args(args)
        .build()
        .map_err(|e| AppError::Message(format!("创建 AutoLaunch 失败: {e}")))
}

/// 登记开机自启。`silent` 决定启动项里带不带 `--silent`。
///
/// 先删后写：部分平台（macOS 登录项）参数是键的一部分，直接 enable 不会覆盖
/// 旧的参数形态，会出现「改了静默开关但启动项没变」。
///
/// 重复调用安全，且会按当前可执行文件路径重写启动项：升级后安装路径若变了，
/// 启动时按设置重放一次就能把指向旧路径的启动项纠正过来。
pub fn enable_auto_launch(silent: bool) -> Result<(), AppError> {
    let auto_launch = get_auto_launch(silent)?;
    if auto_launch
        .is_enabled()
        .map_err(|e| AppError::Message(format!("检查开机自启状态失败: {e}")))?
    {
        auto_launch
            .disable()
            .map_err(|e| AppError::Message(format!("更新开机自启失败: {e}")))?;
    }
    auto_launch
        .enable()
        .map_err(|e| AppError::Message(format!("启用开机自启失败: {e}")))?;
    log::info!("已启用开机自启（silent={silent}）");
    Ok(())
}

/// 取消开机自启。
///
/// 系统里本来就没登记时直接当成功：`delete_value` 在键不存在时会报错，而
/// 「设置里开着自启、系统里其实没有」（用户自己在任务管理器禁用过）是很常见的
/// 状态，这时点关闭不该弹一个失败提示。
pub fn disable_auto_launch() -> Result<(), AppError> {
    let auto_launch = get_auto_launch(false)?;
    if !auto_launch
        .is_enabled()
        .map_err(|e| AppError::Message(format!("检查开机自启状态失败: {e}")))?
    {
        log::info!("开机自启本就未登记，无需禁用");
        return Ok(());
    }
    auto_launch
        .disable()
        .map_err(|e| AppError::Message(format!("禁用开机自启失败: {e}")))?;
    log::info!("已禁用开机自启");
    Ok(())
}

/// 查询开机自启当前是否已登记。只认启动项在不在，与 `--silent` 无关。
pub fn is_auto_launch_enabled() -> Result<bool, AppError> {
    get_auto_launch(false)?
        .is_enabled()
        .map_err(|e| AppError::Message(format!("检查开机自启状态失败: {e}")))
}

/// 本次进程是不是被「开机自启 + 静默启动」拉起来的。
pub fn started_with_silent_arg() -> bool {
    std::env::args().any(|arg| arg == SILENT_ARG)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 启动项名必须与完整版不同，否则两个应用会互相覆盖对方的自启设置。
    #[test]
    fn app_name_does_not_collide_with_full_version() {
        assert_eq!(APP_NAME, "CC Switch Lite");
        assert_ne!(APP_NAME, "CC Switch");
    }

    /// 静默参数用长选项形式，避免和未来可能出现的短选项撞车。
    #[test]
    fn silent_arg_is_a_long_flag() {
        assert_eq!(SILENT_ARG, "--silent");
        assert!(SILENT_ARG.starts_with("--"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_bundle_path_is_trimmed_to_the_app() {
        let exe =
            std::path::Path::new("/Applications/CC Switch Lite.app/Contents/MacOS/CC Switch Lite");
        assert_eq!(
            macos_app_bundle_path(exe),
            Some(std::path::PathBuf::from("/Applications/CC Switch Lite.app"))
        );
        assert_eq!(
            macos_app_bundle_path(std::path::Path::new("/usr/local/bin/x")),
            None
        );
    }

    /// 真去读写系统启动项，跑完自己清理。
    ///
    /// 默认 `#[ignore]`：CI 与普通 `cargo test` 都不该碰用户的注册表 / 登录项。
    /// 需要人工核验时跑 `cargo test --lib -- --ignored auto_launch_registry`。
    ///
    /// 注意本测试用的是正在跑的测试可执行文件路径，所以验收的是「登记与撤销」这套
    /// 机制本身，不去断言登记结果指向哪个程序。
    #[test]
    #[ignore = "touches the real HKCU Run key / login items"]
    fn auto_launch_registry_round_trip() {
        // 先清干净，避免上一次中断留下的残留影响判断
        let _ = disable_auto_launch();
        assert!(
            !is_auto_launch_enabled().unwrap(),
            "清理后不应仍处于已启用状态"
        );

        enable_auto_launch(true).expect("启用开机自启");
        assert!(is_auto_launch_enabled().unwrap(), "启用后应能查到已登记");

        // 静默开关切换要能改写启动项，而不是留下旧参数
        enable_auto_launch(false).expect("改写为不带 --silent");
        assert!(
            is_auto_launch_enabled().unwrap(),
            "改写参数后仍应处于已启用"
        );

        disable_auto_launch().expect("禁用开机自启");
        assert!(!is_auto_launch_enabled().unwrap(), "禁用后不应再查到");

        // 重复禁用要幂等：系统里没有时不该报错
        disable_auto_launch().expect("重复禁用应当成功");
    }
}
