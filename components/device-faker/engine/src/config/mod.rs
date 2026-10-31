use anyhow::Result;
use indexmap::IndexMap;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

mod profile;
mod properties;
#[cfg(test)]
mod tests;
use profile::*;
pub use profile::{DPI_MAX, DPI_MIN, NumberSetting, is_patch_only_key, parse_sdk_override};

/// 机型模板
#[derive(Debug, Clone, Deserialize)]
pub struct DeviceTemplate {
    /// 包名列表
    #[serde(default)]
    pub packages: Vec<String>,
    /// 设备信息
    #[serde(default)]
    pub manufacturer: Option<String>,
    #[serde(default)]
    pub brand: Option<String>,
    #[serde(default)]
    pub marketname: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub device: Option<String>,
    #[serde(default)]
    pub product: Option<String>,
    #[serde(default)]
    pub hardware: Option<String>,
    #[serde(default)]
    pub board: Option<String>,
    /// SoC 型号伪装（映射 Build.SOC_MODEL + ro.soc.model，单属性无分区副本）
    #[serde(default)]
    pub soc_model: Option<String>,
    #[serde(default)]
    pub fingerprint: Option<String>,
    #[serde(default)]
    pub build_id: Option<String>,
    /// 系统版本号（映射 Build.DISPLAY + ro.build.display.id，单属性无分区副本）
    #[serde(default)]
    pub display_id: Option<String>,
    /// 构建增量号（映射 Build.VERSION.INCREMENTAL + ro.build.version.incremental 族）
    #[serde(default)]
    pub incremental: Option<String>,
    /// 安全补丁日期（映射 Build.VERSION.SECURITY_PATCH + ro.build.version.security_patch 族）
    #[serde(default)]
    pub security_patch: Option<String>,
    #[serde(default)]
    pub characteristics: Option<String>,
    /// Android 版本伪装（如 "15", "14"）
    #[serde(default)]
    pub android_version: Option<String>,
    /// SDK 版本伪装（如 35, 34）；`"__DELETE__"` 删除 `ro.build.version.sdk` 整族
    #[serde(default)]
    pub sdk_int: Option<NumberSetting>,
    /// 临时覆盖系统显示密度（120–640），由 companion 使用显式用户密度接口应用
    #[serde(default)]
    pub dpi: Option<u32>,
    /// 自定义属性映射表
    #[serde(default)]
    pub custom_props: Option<HashMap<String, String>>,
    /// 是否为匹配的应用强制执行 FORCE_DENYLIST_UNMOUNT（默认继承全局设置）
    #[serde(default)]
    pub force_denylist_unmount: Option<bool>,
    /// 要从 /proc/self/maps 中清除的属性映射模式列表（默认继承全局设置）
    #[serde(default)]
    pub hide_maps: Option<Vec<String>>,
    /// 是否跳过 COW 属性伪造，所有属性直接交给 companion resetprop 处理
    /// true 时 getprop（独立进程）和进程内读取一致，适用于属性一致性对比检测
    #[serde(default)]
    pub companion_resetprop: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub package: String,
    /// 直接指定设备信息
    #[serde(default)]
    pub manufacturer: Option<String>,
    #[serde(default)]
    pub brand: Option<String>,
    #[serde(default)]
    pub marketname: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub device: Option<String>,
    #[serde(default)]
    pub product: Option<String>,
    #[serde(default)]
    pub hardware: Option<String>,
    #[serde(default)]
    pub board: Option<String>,
    /// SoC 型号伪装（映射 Build.SOC_MODEL + ro.soc.model，单属性无分区副本）
    #[serde(default)]
    pub soc_model: Option<String>,
    #[serde(default)]
    pub fingerprint: Option<String>,
    #[serde(default)]
    pub build_id: Option<String>,
    /// 系统版本号（映射 Build.DISPLAY + ro.build.display.id，单属性无分区副本）
    #[serde(default)]
    pub display_id: Option<String>,
    /// 构建增量号（映射 Build.VERSION.INCREMENTAL + ro.build.version.incremental 族）
    #[serde(default)]
    pub incremental: Option<String>,
    /// 安全补丁日期（映射 Build.VERSION.SECURITY_PATCH + ro.build.version.security_patch 族）
    #[serde(default)]
    pub security_patch: Option<String>,
    #[serde(default)]
    pub characteristics: Option<String>,
    /// Android 版本伪装（如 "15", "14"）
    #[serde(default)]
    pub android_version: Option<String>,
    /// SDK 版本伪装（如 35, 34）；`"__DELETE__"` 删除 `ro.build.version.sdk` 整族
    #[serde(default)]
    pub sdk_int: Option<NumberSetting>,
    /// 临时覆盖系统显示密度（120–640），由 companion 使用显式用户密度接口应用
    #[serde(default)]
    pub dpi: Option<u32>,
    /// 自定义属性映射表
    #[serde(default)]
    pub custom_props: Option<HashMap<String, String>>,
    /// 是否为该应用强制执行 FORCE_DENYLIST_UNMOUNT（默认继承全局设置）
    #[serde(default)]
    pub force_denylist_unmount: Option<bool>,
    /// 要从 /proc/self/maps 中清除的属性映射模式列表（默认继承全局设置）
    #[serde(default)]
    pub hide_maps: Option<Vec<String>>,
    /// 是否跳过 COW 属性伪造，所有属性直接交给 companion resetprop 处理
    /// true 时 getprop（独立进程）和进程内读取一致，适用于属性一致性对比检测
    #[serde(default)]
    pub companion_resetprop: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    /// 是否默认启用 FORCE_DENYLIST_UNMOUNT（避免模块挂载痕迹）
    #[serde(default)]
    pub default_force_denylist_unmount: bool,
    /// 是否启用调试日志（默认关闭以提高隐蔽性）
    #[serde(default)]
    pub debug: bool,
    /// 机型设备模板定义
    ///
    /// 用 IndexMap 而非 HashMap：保留 config.toml 中的书写顺序，使
    /// find_template_for_package 在「同一包名被多个模板声明」时命中确定的
    /// 那一个（书写顺序靠前者），而不是 HashMap 的进程级随机迭代顺序。
    #[serde(default)]
    pub templates: IndexMap<String, DeviceTemplate>,
    /// 应用配置
    #[serde(default)]
    pub apps: Vec<AppConfig>,
    /// 要从 /proc/self/maps 中清除的属性映射模式列表（默认不启用）
    #[serde(default)]
    pub default_hide_maps: Vec<String>,
}

impl Config {
    pub fn from_toml(content: &str) -> Result<Self> {
        let value: toml::Value = toml::from_str(content)?;
        if contains_nul(&value) {
            return Err(ConfigValidationError::Invalid(
                "Configuration cannot contain NUL characters",
            )
            .into());
        }
        let config: Self = value.try_into()?;
        for (global, props) in config
            .apps
            .iter()
            .map(|app| (app.companion_resetprop, &app.custom_props))
            .chain(
                config
                    .templates
                    .values()
                    .map(|template| (template.companion_resetprop, &template.custom_props)),
            )
        {
            if global == Some(true)
                && props
                    .as_ref()
                    .is_some_and(|props| props.keys().any(|key| !key.starts_with("ro.")))
            {
                return Err(ConfigValidationError::Invalid("Global property profiles support ro.* keys only; mutable properties require private COW mode").into());
            }
        }
        for (sdk, dpi) in config.apps.iter().map(|app| (app.sdk_int, app.dpi)).chain(
            config
                .templates
                .values()
                .map(|template| (template.sdk_int, template.dpi)),
        ) {
            profile::validate_numbers(sdk, dpi)?;
        }
        for props in config
            .apps
            .iter()
            .filter_map(|app| app.custom_props.as_ref())
            .chain(
                config
                    .templates
                    .values()
                    .filter_map(|template| template.custom_props.as_ref()),
            )
        {
            properties::validate_custom_props(props)?;
        }
        Ok(config)
    }

    /// 查找包名对应的应用配置（优先）或模板配置
    pub fn get_app_config(&self, package_name: &str) -> Option<&AppConfig> {
        self.apps.iter().find(|app| app.package == package_name)
    }

    /// 查找包名对应的模板（从模板的 packages 列表中查找）
    ///
    /// 多个模板声明同一包名时命中**书写顺序最先**的那一个。Config.templates 是
    /// IndexMap（配合 toml 的 preserve_order），顺序即 config.toml 中的出现顺序，
    /// 因此结果是确定的、跨进程一致的——不依赖 HashMap 的随机迭代顺序。
    pub fn find_template_for_package(&self, package_name: &str) -> Option<&DeviceTemplate> {
        self.templates
            .iter()
            .find(|(_, template)| template.packages.iter().any(|pkg| pkg == package_name))
            .map(|(_, template)| template)
    }

    /// 找出所有声明了该包名的模板名（按书写顺序）。
    ///
    /// 长度 > 1 表示配置存在歧义：只有第一个会真正生效，其余被静默丢弃。
    /// 用于诊断日志/告警，调用方可据此提示用户「一个包只应挂一个模板」。
    pub fn conflicting_templates_for_package(&self, package_name: &str) -> Vec<&str> {
        self.templates
            .iter()
            .filter(|(_, template)| template.packages.iter().any(|pkg| pkg == package_name))
            .map(|(name, _)| name.as_str())
            .collect()
    }

    /// 获取应用的最终配置（优先查找直接配置，其次查找模板的 packages 列表）
    pub fn get_merged_config(&self, package_name: &str) -> Option<MergedAppConfig> {
        // 优先查找直接配置的应用
        if let Some(app) = self.get_app_config(package_name) {
            let mut merged = MergedAppConfig {
                manufacturer: app.manufacturer.clone(),
                brand: app.brand.clone(),
                marketname: app.marketname.clone(),
                model: app.model.clone(),
                name: app.name.clone(),
                device: app.device.clone(),
                product: app.product.clone(),
                hardware: app.hardware.clone(),
                board: app.board.clone(),
                soc_model: app.soc_model.clone(),
                fingerprint: app.fingerprint.clone(),
                build_id: app.build_id.clone(),
                display_id: app.display_id.clone(),
                incremental: app.incremental.clone(),
                security_patch: app.security_patch.clone(),
                characteristics: app.characteristics.clone(),
                android_version: app.android_version.clone(),
                sdk_int: app.sdk_int,
                dpi: app.dpi,
                custom_props: app.custom_props.clone(),
                force_denylist_unmount: app
                    .force_denylist_unmount
                    .unwrap_or(self.default_force_denylist_unmount),
                hide_maps: app
                    .hide_maps
                    .clone()
                    .unwrap_or_else(|| self.default_hide_maps.clone()),
                companion_resetprop: app.companion_resetprop.unwrap_or(false),
                derived_props: HashSet::new(),
            };
            merged.complete_device_profile();
            return Some(merged);
        }

        // 如果没有直接配置，查找模板的 packages 列表
        if let Some(template) = self.find_template_for_package(package_name) {
            let mut merged = MergedAppConfig {
                manufacturer: template.manufacturer.clone(),
                brand: template.brand.clone(),
                marketname: template.marketname.clone(),
                model: template.model.clone(),
                name: template.name.clone(),
                device: template.device.clone(),
                product: template.product.clone(),
                hardware: template.hardware.clone(),
                board: template.board.clone(),
                soc_model: template.soc_model.clone(),
                fingerprint: template.fingerprint.clone(),
                build_id: template.build_id.clone(),
                display_id: template.display_id.clone(),
                incremental: template.incremental.clone(),
                security_patch: template.security_patch.clone(),
                characteristics: template.characteristics.clone(),
                android_version: template.android_version.clone(),
                sdk_int: template.sdk_int,
                dpi: template.dpi,
                custom_props: template.custom_props.clone(),
                force_denylist_unmount: template
                    .force_denylist_unmount
                    .unwrap_or(self.default_force_denylist_unmount),
                hide_maps: template
                    .hide_maps
                    .clone()
                    .unwrap_or_else(|| self.default_hide_maps.clone()),
                companion_resetprop: template.companion_resetprop.unwrap_or(false),
                derived_props: HashSet::new(),
            };
            merged.complete_device_profile();
            return Some(merged);
        }

        None
    }
}

/// 合并后的应用配置（模板 + 直接配置）
#[derive(Debug, Clone)]
pub struct MergedAppConfig {
    pub manufacturer: Option<String>,
    pub brand: Option<String>,
    pub marketname: Option<String>,
    pub model: Option<String>,
    pub name: Option<String>,
    pub device: Option<String>,
    pub product: Option<String>,
    pub hardware: Option<String>,
    pub board: Option<String>,
    pub soc_model: Option<String>,
    pub fingerprint: Option<String>,
    pub build_id: Option<String>,
    /// 系统版本号 → Build.DISPLAY + ro.build.display.id
    pub display_id: Option<String>,
    /// 构建增量号 → Build.VERSION.INCREMENTAL + ro.build.version.incremental 族
    pub incremental: Option<String>,
    /// 安全补丁日期 → Build.VERSION.SECURITY_PATCH + ro.build.version.security_patch 族
    pub security_patch: Option<String>,
    pub characteristics: Option<String>,
    pub android_version: Option<String>,
    pub sdk_int: Option<NumberSetting>,
    /// 最终显示密度覆盖值，超出 120–640 的配置会被忽略
    pub dpi: Option<u32>,
    pub custom_props: Option<HashMap<String, String>>,
    pub force_denylist_unmount: bool,
    /// 要从 /proc/self/maps 中清除的属性映射模式列表
    pub hide_maps: Vec<String>,
    /// 是否跳过 COW，所有属性走 companion resetprop（默认 false）
    pub companion_resetprop: bool,
    /// 模块**自行注入** `custom_props` 的派生键（目前只有指纹解析出的
    /// `ro.build.type` / `ro.build.tags`，见 `complete_device_profile`）。
    /// 这些键不算「用户显式写出」，因此**不享受** patch-only 门控豁免
    /// （见 `build_merged_property_map_with_explicit` 的 `explicit` 集合）。
    pub derived_props: HashSet<String>,
}

impl MergedAppConfig {
    /// 用构建指纹补齐未显式填写的标准字段，并同步指纹内携带的构建信息。
    ///
    /// 显式模板字段始终优先；指纹只负责补空，不会覆盖用户主动设置的
    /// 品牌、设备代号等。指纹携带的 incremental/type/tags 以 custom_props
    /// 条目形式补入（不覆盖用户显式配置），随后按属性族展开生效。
    fn complete_device_profile(&mut self) {
        let parsed = self
            .fingerprint
            .as_deref()
            .and_then(FingerprintParts::parse);

        if let Some(parts) = &parsed {
            fill_profile_default(&mut self.brand, parts.brand.clone());
            fill_profile_default(&mut self.product, parts.product.clone());
            fill_profile_default(&mut self.device, parts.device.clone());
            fill_profile_default(&mut self.android_version, parts.release.clone());
            fill_profile_default(&mut self.build_id, parts.build_id.clone());
            // 指纹 incremental 同步进专用字段，保证 Build.VERSION.INCREMENTAL 与
            // ro.build.version.incremental 一致（原先只进 custom_props 会漏 hook）
            fill_profile_default(&mut self.incremental, parts.incremental.clone());
        }

        // Build.PRODUCT 的属性来源是 ro.product.name，而本项目的 product 与
        // name 语义相同（产品代号）。两者必须同步，否则 JNI 字段与系统属性
        // 会分别显示新旧值。
        if !has_profile_value(&self.product)
            && let Some(name) = self.name.as_ref().filter(|value| !value.is_empty())
        {
            self.product = Some(name.clone());
        }
        if !has_profile_value(&self.name)
            && let Some(product) = self.product.as_ref().filter(|value| !value.is_empty())
        {
            self.name = Some(product.clone());
        }
        if !has_profile_value(&self.device)
            && let Some(product) = self.product.as_ref().filter(|value| !value.is_empty())
        {
            self.device = Some(product.clone());
        }

        if let Some(parts) = parsed {
            let props = self.custom_props.get_or_insert_with(HashMap::new);
            // 指纹派生键：用户没显式写时才注入，并登记为「派生」（不受 patch-only
            // 门控豁免，理由见 MergedAppConfig::derived_props）。
            for (key, value) in [
                ("ro.build.type", parts.build_type),
                ("ro.build.tags", parts.tags),
            ] {
                if !props.contains_key(key) {
                    props.insert(key.to_string(), value);
                    self.derived_props.insert(key.to_string());
                }
            }
        }
    }
}

#[derive(Debug)]
pub enum ConfigValidationError {
    Invalid(&'static str),
    DensityRange,
}
impl std::fmt::Display for ConfigValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Invalid(message) => message,
            Self::DensityRange => "DPI must be between 120 and 640",
        })
    }
}
impl std::error::Error for ConfigValidationError {}
fn contains_nul(value: &toml::Value) -> bool {
    match value {
        toml::Value::String(text) => text.contains('\0'),
        toml::Value::Array(values) => values.iter().any(contains_nul),
        toml::Value::Table(table) => table
            .iter()
            .any(|(key, value)| key.contains('\0') || contains_nul(value)),
        _ => false,
    }
}
