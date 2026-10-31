use anyhow::Context;
use jni::{
    Env, EnvUnowned, jni_sig, jni_str,
    objects::{JClass, JValue},
    strings::JNIStr,
};

use std::collections::HashMap;

use crate::config::parse_sdk_override;

/// 根据最终属性计划更新 android.os.Build 的静态字段。
pub fn hook_build_fields(
    env: &mut EnvUnowned,
    props: &HashMap<String, String>,
) -> anyhow::Result<()> {
    let sdk = props
        .get("ro.build.version.sdk")
        .map(|value| parse_sdk_override(value).map(|sdk| (value.as_str(), sdk)))
        .transpose()?;
    let completed = env
        .with_env(|jenv| -> anyhow::Result<bool> {
            if jenv.exception_check() {
                return Ok(false);
            }
            let build_class = jenv.find_class(jni_str!("android/os/Build"))?;

            for (field, value) in [
                (jni_str!("MANUFACTURER"), "ro.product.manufacturer"),
                (jni_str!("BRAND"), "ro.product.brand"),
                (jni_str!("MODEL"), "ro.product.model"),
                (jni_str!("DEVICE"), "ro.product.device"),
                (jni_str!("PRODUCT"), "ro.product.name"),
                (jni_str!("HARDWARE"), "ro.hardware"),
                (jni_str!("BOARD"), "ro.product.board"),
                // SOC_MODEL exists starting at API 31; typed missing-field errors skip it.
                (jni_str!("SOC_MODEL"), "ro.soc.model"),
                (jni_str!("FINGERPRINT"), "ro.build.fingerprint"),
                (jni_str!("ID"), "ro.build.id"),
                (jni_str!("DISPLAY"), "ro.build.display.id"),
            ] {
                if let Some(value) = props.get(value) {
                    set_build_field(jenv, &build_class, field, value)?;
                }
            }
            hook_version_fields(jenv, props, sdk)?;

            Ok(true)
        })
        .resolve::<crate::jni_policy::ClearAndDefault>();
    anyhow::ensure!(completed, "Build fields could not be applied");
    Ok(())
}

fn hook_version_fields(
    env: &mut Env,
    props: &HashMap<String, String>,
    sdk: Option<(&str, i32)>,
) -> anyhow::Result<()> {
    let version_class = env
        .find_class(jni_str!("android/os/Build$VERSION"))
        .context("Failed to find Build.VERSION class")?;

    if let Some(android_version) = props.get("ro.build.version.release") {
        set_build_field(env, &version_class, jni_str!("RELEASE"), android_version)?;
    }

    // Deleted keys are absent from the final plan; their Java fields remain unchanged.
    if let Some((sdk_string, sdk_int)) = sdk {
        set_build_int_field(env, &version_class, jni_str!("SDK_INT"), sdk_int)?;
        set_build_field(env, &version_class, jni_str!("SDK"), sdk_string)?;
    }

    // Build.VERSION.INCREMENTAL 的属性来源是 ro.build.version.incremental
    if let Some(incremental) = props.get("ro.build.version.incremental") {
        set_build_field(env, &version_class, jni_str!("INCREMENTAL"), incremental)?;
    }

    // Build.VERSION.SECURITY_PATCH 的属性来源是 ro.build.version.security_patch
    if let Some(security_patch) = props.get("ro.build.version.security_patch") {
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
