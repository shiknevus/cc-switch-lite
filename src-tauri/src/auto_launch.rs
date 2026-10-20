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

/// 启动项名字。刻意与完整版的 "CC Switch" 区分，两边各自独立启停。
pub const APP_NAME: &str = "CC Switch Lite";

/// 开机自启时附加的参数：只驻留托盘，不显示主窗口。
pub const SILENT_ARG: &str = "--silent";

#[cfg(target_os = "windows")]
mod platform {
    use super::*;
    use winreg::enums::{RegType, HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE};
    use winreg::{RegKey, RegValue};

    const RUN_KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run";
    const STARTUP_APPROVED_KEY: &str =
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
    const TASK_MANAGER_OVERRIDE_ENABLED_VALUE: [u8; 12] = [
        0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    pub fn enable(silent: bool) -> Result<(), AppError> {
        let exe_path = std::env::current_exe()
            .map_err(|e| AppError::Message(format!("无法获取当前程序路径: {e}")))?;

        // Windows 注册表 Run 键规则：可执行文件路径如果包含空格（如 "CC Switch Lite"），
        // 必须严格使用英文双引号包裹！否则系统开机时将路径空格截断解析，导致自启失败！
        let cmd = if silent {
            format!("\"{}\" {}", exe_path.display(), SILENT_ARG)
        } else {
            format!("\"{}\"", exe_path.display())
        };

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (run_key, _) = hkcu
            .create_subkey_with_flags(RUN_KEY, KEY_SET_VALUE)
            .map_err(|e| AppError::Message(format!("打开注册表 Run 键失败: {e}")))?;

        run_key
            .set_value(APP_NAME, &cmd)
            .map_err(|e| AppError::Message(format!("写入开机自启注册表失败: {e}")))?;

        // 确保未被 Windows 任务管理器禁用（StartupApproved 写入 0x02 启用态）
        if let Ok(reg) = hkcu.open_subkey_with_flags(STARTUP_APPROVED_KEY, KEY_SET_VALUE) {
            let _ = reg.set_raw_value(
                APP_NAME,
                &RegValue {
                    vtype: RegType::REG_BINARY,
                    bytes: TASK_MANAGER_OVERRIDE_ENABLED_VALUE.to_vec(),
                },
            );
        }

        log::info!("Windows 注册表开机自启已登记: {cmd}");
        Ok(())
    }

    pub fn disable() -> Result<(), AppError> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(run_key) = hkcu.open_subkey_with_flags(RUN_KEY, KEY_SET_VALUE) {
            let _ = run_key.delete_value(APP_NAME);
        }
        log::info!("Windows 注册表开机自启已清除: {APP_NAME}");
        Ok(())
    }

    pub fn is_enabled() -> Result<bool, AppError> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let run_key = match hkcu.open_subkey_with_flags(RUN_KEY, KEY_READ) {
            Ok(k) => k,
            Err(_) => return Ok(false),
        };

        if run_key.get_value::<String, _>(APP_NAME).is_err() {
            return Ok(false);
        }

        // 检查任务管理器 StartupApproved 状态
        if let Ok(approved_key) = hkcu.open_subkey_with_flags(STARTUP_APPROVED_KEY, KEY_READ) {
            if let Ok(raw_val) = approved_key.get_raw_value(APP_NAME) {
                // 第一个字节为奇数（如 0x03）表示被用户在任务管理器中显式禁用
                if raw_val.bytes.first().map_or(false, |&b| b % 2 != 0) {
                    return Ok(false);
                }
            }
        }

        Ok(true)
    }
}

#[cfg(not(target_os = "windows"))]
mod platform {
    use super::*;
    use auto_launch::{AutoLaunch, AutoLaunchBuilder};

    #[cfg(target_os = "macos")]
    fn macos_app_bundle_path(exe_path: &std::path::Path) -> Option<std::path::PathBuf> {
        let path_str = exe_path.to_string_lossy();
        let app_pos = path_str.find(".app/Contents/MacOS/")?;
        Some(std::path::PathBuf::from(&path_str[..app_pos + 4]))
    }

    fn get_auto_launch(silent: bool) -> Result<AutoLaunch, AppError> {
        let exe_path = std::env::current_exe()
            .map_err(|e| AppError::Message(format!("无法获取应用路径: {e}")))?;

        #[cfg(target_os = "macos")]
        let app_path = macos_app_bundle_path(&exe_path).unwrap_or(exe_path);

        #[cfg(not(target_os = "macos"))]
        let app_path = exe_path;

        let args: &[&str] = if silent { &[SILENT_ARG] } else { &[] };

        AutoLaunchBuilder::new()
            .set_app_name(APP_NAME)
            .set_app_path(&app_path.to_string_lossy())
            .set_args(args)
            .build()
            .map_err(|e| AppError::Message(format!("创建 AutoLaunch 失败: {e}")))
    }

    pub fn enable(silent: bool) -> Result<(), AppError> {
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

    pub fn disable() -> Result<(), AppError> {
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

    pub fn is_enabled() -> Result<bool, AppError> {
        get_auto_launch(false)?
            .is_enabled()
            .map_err(|e| AppError::Message(format!("检查开机自启状态失败: {e}")))
    }
}

pub fn enable_auto_launch(silent: bool) -> Result<(), AppError> {
    platform::enable(silent)
}

pub fn disable_auto_launch() -> Result<(), AppError> {
    platform::disable()
}

pub fn is_auto_launch_enabled() -> Result<bool, AppError> {
    platform::is_enabled()
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

        #[cfg(target_os = "windows")]
        {
            use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};
            use winreg::RegKey;
            let hkcu = RegKey::predef(HKEY_CURRENT_USER);
            let run_key = hkcu
                .open_subkey_with_flags(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run", KEY_READ)
                .unwrap();
            let val: String = run_key.get_value(APP_NAME).unwrap();
            assert!(
                val.starts_with('"') && val.contains("\" --silent"),
                "必须带有双引号与参数: {val}"
            );
        }

        enable_auto_launch(false).expect("改写为不带 --silent");
        assert!(
            is_auto_launch_enabled().unwrap(),
            "改写参数后仍应处于已启用"
        );

        #[cfg(target_os = "windows")]
        {
            use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};
            use winreg::RegKey;
            let hkcu = RegKey::predef(HKEY_CURRENT_USER);
            let run_key = hkcu
                .open_subkey_with_flags(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run", KEY_READ)
                .unwrap();
            let val: String = run_key.get_value(APP_NAME).unwrap();
            assert!(
                val.starts_with('"') && val.ends_with('"'),
                "不带参数时两端应为双引号包裹，无多余空格: {val}"
            );
        }

        disable_auto_launch().expect("禁用开机自启");
        assert!(!is_auto_launch_enabled().unwrap(), "禁用后不应再查到");
        disable_auto_launch().expect("重复禁用应当成功");
    }
}
