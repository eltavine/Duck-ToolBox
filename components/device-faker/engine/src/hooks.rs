use anyhow::Context;
use jni::{
    Env, EnvUnowned, jni_sig, jni_str,
    objects::{JClass, JValue},
    strings::JNIStr,
};

use crate::config::{MergedAppConfig, NumberSetting};

/// Build 字段的有效伪装值：与 config::field_value 相同的归一化——
/// 未设置/空串/`__DELETE__` 返回 None（跳过 hook），`__EMPTY__` 归一化为空串。
fn field_str(field: &Option<String>) -> Option<&str> {
    match field.as_deref() {
        Some(value) if !value.is_empty() && value != "__DELETE__" => {
            Some(if value == "__EMPTY__" { "" } else { value })
        }
        _ => None,
    }
}

/// 根据合并配置 Hook android.os.Build 的静态字段。
pub fn hook_build_fields(
    env: &mut EnvUnowned,
    merged_config: &MergedAppConfig,
) -> anyhow::Result<()> {
    let completed = env
        .with_env(|jenv| -> anyhow::Result<bool> {
            if jenv.exception_check() {
                return Ok(false);
            }
            let build_class = jenv.find_class(jni_str!("android/os/Build"))?;

            for (field, value) in [
                (jni_str!("MANUFACTURER"), &merged_config.manufacturer),
                (jni_str!("BRAND"), &merged_config.brand),
                (jni_str!("MODEL"), &merged_config.model),
                (jni_str!("DEVICE"), &merged_config.device),
                (jni_str!("PRODUCT"), &merged_config.product),
                (jni_str!("HARDWARE"), &merged_config.hardware),
                (jni_str!("BOARD"), &merged_config.board),
                // SOC_MODEL exists starting at API 31; typed missing-field errors skip it.
                (jni_str!("SOC_MODEL"), &merged_config.soc_model),
                (jni_str!("FINGERPRINT"), &merged_config.fingerprint),
                (jni_str!("ID"), &merged_config.build_id),
                (jni_str!("DISPLAY"), &merged_config.display_id),
            ] {
                if let Some(value) = field_str(value) {
                    set_build_field(jenv, &build_class, field, value)?;
                }
            }
            hook_version_fields(jenv, &build_class, merged_config)?;

            Ok(true)
        })
        .resolve::<crate::jni_policy::ClearAndDefault>();
    anyhow::ensure!(completed, "Build fields could not be applied");
    Ok(())
}

fn hook_version_fields(
    env: &mut Env,
    _build_class: &JClass,
    merged_config: &MergedAppConfig,
) -> anyhow::Result<()> {
    let version_class = env
        .find_class(jni_str!("android/os/Build$VERSION"))
        .context("Failed to find Build.VERSION class")?;

    if let Some(android_version) = field_str(&merged_config.android_version) {
        set_build_field(env, &version_class, jni_str!("RELEASE"), android_version)?;
    }

    // `__DELETE__`（NumberSetting::Delete）不 hook 该字段：与字符串字段的
    // `__DELETE__` 语义一致——只由 delete_props 列表在 companion 侧删除属性，
    // Java 层字段保持系统原值。
    if let Some(sdk_int) = merged_config.sdk_int.and_then(NumberSetting::value) {
        set_build_int_field(
            env,
            &version_class,
            jni_str!("SDK_INT"),
            i32::try_from(sdk_int).context("SDK_INT exceeds Java integer range")?,
        )?;
    }

    // Build.VERSION.INCREMENTAL 的属性来源是 ro.build.version.incremental
    if let Some(incremental) = field_str(&merged_config.incremental) {
        set_build_field(env, &version_class, jni_str!("INCREMENTAL"), incremental)?;
    }

    // Build.VERSION.SECURITY_PATCH 的属性来源是 ro.build.version.security_patch
    if let Some(security_patch) = field_str(&merged_config.security_patch) {
        set_build_field(
            env,
            &version_class,
            jni_str!("SECURITY_PATCH"),
            security_patch,
        )?;
    }

    Ok(())
}

fn set_build_field(
    env: &mut Env,
    build_class: &JClass,
    field_name: &JNIStr,
    value: &str,
) -> anyhow::Result<()> {
    if let Err(error) =
        env.get_static_field_id(build_class, field_name, jni_sig!("Ljava/lang/String;"))
    {
        if matches!(error, jni::errors::Error::FieldNotFound { .. }) {
            return Ok(());
        }
        return Err(error).context("Failed to get field ID");
    }

    let new_value = env
        .new_string(value)
        .with_context(|| format!("Failed to create string for {value}"))?;

    env.set_static_field(
        build_class,
        field_name,
        jni_sig!("Ljava/lang/String;"),
        JValue::Object(&new_value),
    )
    .with_context(|| "Failed to set field".to_string())?;

    Ok(())
}

fn set_build_int_field(
    env: &mut Env,
    build_class: &JClass,
    field_name: &JNIStr,
    value: i32,
) -> anyhow::Result<()> {
    if let Err(error) = env.get_static_field_id(build_class, field_name, jni_sig!("I")) {
        if matches!(error, jni::errors::Error::FieldNotFound { .. }) {
            return Ok(());
        }
        return Err(error).context("Failed to get field ID");
    }

    env.set_static_field(build_class, field_name, jni_sig!("I"), JValue::Int(value))
        .with_context(|| "Failed to set field".to_string())?;

    Ok(())
}

#[cfg(all(test, unix, not(target_os = "android")))]
#[path = "hooks/tests.rs"]
mod tests;
