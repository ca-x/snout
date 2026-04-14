use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// 内置主题定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkinPreset {
    pub key: String,
    pub display_name: String,
    pub author: String,
    pub values: HashMap<String, serde_yaml::Value>,
}

/// 获取所有内置主题
pub fn builtin_skins() -> Vec<SkinPreset> {
    vec![
        SkinPreset {
            key: "jianchun".into(),
            display_name: "简纯".into(),
            author: "amzxyz".into(),
            values: map![
                "name".into() => "简纯".into(),
                "back_color".into() => "0xf2f2f2".into(),
                "border_color".into() => "0xCE7539".into(),
                "text_color".into() => "0x3c647e".into(),
                "hilited_text_color".into() => "0x3c647e".into(),
                "hilited_back_color".into() => "0x797954".into(),
                "hilited_candidate_back_color".into() => "0xCE7539".into(),
                "candidate_text_color".into() => "0x000000".into(),
            ],
        },
        SkinPreset {
            key: "win11_light".into(),
            display_name: "Win11浅色".into(),
            author: "community".into(),
            values: map![
                "name".into() => "Win11浅色".into(),
                "text_color".into() => "0x191919".into(),
                "back_color".into() => "0xf9f9f9".into(),
                "border_color".into() => "0x009e5a00".into(),
                "hilited_mark_color".into() => "0xc06700".into(),
                "hilited_candidate_back_color".into() => "0xf0f0f0".into(),
            ],
        },
        SkinPreset {
            key: "win11_dark".into(),
            display_name: "Win11暗色".into(),
            author: "community".into(),
            values: map![
                "name".into() => "Win11暗色".into(),
                "text_color".into() => "0xf9f9f9".into(),
                "back_color".into() => "0x2C2C2C".into(),
                "hilited_mark_color".into() => "0xFFC24C".into(),
                "hilited_candidate_back_color".into() => "0x383838".into(),
            ],
        },
        SkinPreset {
            key: "wechat".into(),
            display_name: "微信".into(),
            author: "community".into(),
            values: map![
                "name".into() => "微信".into(),
                "text_color".into() => "0x424242".into(),
                "back_color".into() => "0xFFFFFF".into(),
                "hilited_back_color".into() => "0x79af22".into(),
                "hilited_candidate_back_color".into() => "0x79af22".into(),
            ],
        },
        SkinPreset {
            key: "mac_light".into(),
            display_name: "Mac 白".into(),
            author: "community".into(),
            values: map![
                "name".into() => "Mac 白".into(),
                "text_color".into() => "0x000000".into(),
                "back_color".into() => "0xffffff".into(),
                "hilited_candidate_back_color".into() => serde_yaml::Value::Number(16740656.into()),
            ],
        },
        SkinPreset {
            key: "reimu".into(),
            display_name: "灵梦".into(),
            author: "Lufs X".into(),
            values: map![
                "name".into() => "灵梦".into(),
                "back_color".into() => "0xF5FCFD".into(),
                "text_color".into() => "0x6B54E9".into(),
                "hilited_candidate_back_color".into() => "0xF5FCFD".into(),
                "hilited_candidate_text_color".into() => "0x4F00E5".into(),
            ],
        },
    ]
}

/// 通过 key 查找主题
pub fn find_skin(key: &str) -> Option<SkinPreset> {
    builtin_skins().into_iter().find(|s| s.key == key)
}

/// map! 宏简化 HashMap 创建
macro_rules! map {
    ($($k:expr => $v:expr),* $(,)?) => {{
        let mut m = HashMap::new();
        $(m.insert($k, $v);)*
        m
    }};
}
use map;
