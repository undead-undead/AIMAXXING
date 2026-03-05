//! Internationalization (i18n) module for aimaxxing-panel.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    En,
    Zh,
}

impl Default for Language {
    fn default() -> Self {
        Self::En
    }
}

pub fn t(key: &str, lang: Language) -> &str {
    match lang {
        Language::En => translate_en(key),
        Language::Zh => translate_zh(key),
    }
}

fn translate_en(key: &str) -> &str {
    match key {
        // Tabs
        "tabs.skills" => "Skills",
        "tabs.vault" => "API",
        "tabs.store" => "Store",
        "tabs.sessions" => "Sessions",
        "tabs.cron" => "Cron",
        "tabs.persona" => "Soul",
        "tabs.connection" => "Connection",
        "tabs.chat" => "Chat",
        "tabs.dashboard" => "Dash",
        "tabs.logs" => "Logs",
        "tabs.system" => "System",
        "tabs.channels" => "Channels",
        "tabs.speech" => "Speech",
        "tabs.keys" => "LLM Providers",
        "tabs.comm" => "Communication",

        "speech.openai_tts" => "OpenAI TTS",
        "speech.local_model" => "Local Voice Model",
        "speech.model" => "Model ID",
        "speech.voice" => "Voice Persona",
        "speech.path" => "Model Path",
        "speech.enabled" => "Enabled",

        // Skills Sub-tabs
        "skills.installed" => "Installed",
        "skills.market" => "Store",
        "skills.manual" => "Manual",

        // Manual Install
        "install.title" => "Manual Installation",
        "install.hint" => "Visit skills.sh or clawhub.ai, find a skill, then copy the install command and paste it below.",
        "install.subtitle" => "Install Skill",
        "install.paste_hint" => "Paste the install command from skills.sh or clawhub.ai, or a GitHub URL:",

        // Common Buttons
        "btn.refresh" => "Refresh",
        "btn.clear" => "Clear",
        "btn.save" => "Save",
        "btn.cancel" => "Cancel",
        "btn.add" => "Add",
        "btn.apply" => "Apply",
        "btn.run" => "Run",
        "btn.kill" => "KILL",

        // Dashboard Metrics
        "dashboard.token_usage_title" => "Token Usage & Activity",
        "dashboard.total_tokens" => "Total Tokens",
        "dashboard.prompt_tokens" => "Prompt Tokens",
        "dashboard.completion_tokens" => "Completion Tokens",
        "dashboard.total_calls" => "Total Calls",
        "dashboard.avg_latency" => "Avg Latency",
        "dashboard.call_volume" => "Call Volume Trend",

        "system.diagnostics" => "System Diagnostics",
        "system.run_doctor" => "One-Click Diagnosis",
        "system.status_wall" => "Status Wall",
        "system.emergency_brake" => "STOP ALL TASKS",
        "system.sandboxes" => "Sandbox Management",
        "system.kill_pid" => "Kill PID",
        "system.running_sandboxes" => "Active Sandboxes",

        // Blueprints
        "blueprint.gallery" => "Soul Blueprints",
        "blueprint.apply" => "Apply Template",
        "blueprint.category" => "Category",
        "btn.new_agent" => "New Agent",

        // Connection
        "conn.gateway_url" => "Gateway URL",
        "conn.connected" => "Connected",
        "conn.disconnected" => "Disconnected",
        "conn.connecting" => "Connecting...",
        "misc.connected" => "Connected",
        "misc.disconnected" => "Disconnected",

        // Misc
        "misc.theme" => "Theme",
        "misc.language" => "Language",
        "misc.searching" => "Searching...",
        "misc.no_data" => "No data available",
        "soul.export" => "Export Vessel",
        "soul.memory_depth" => "Memory Depth",
        "soul.export_success" => "Vessel exported successfully!",
        "soul.export_failed" => "Export failed",

        _ => key,
    }
}

fn translate_zh(key: &str) -> &str {
    match key {
        // Tabs
        "tabs.skills" => "Skills",
        "tabs.vault" => "API",
        "tabs.logs" => "日志",
        "tabs.store" => "商店",
        "tabs.sessions" => "会话",
        "tabs.cron" => "任务",
        "tabs.persona" => "灵魂",
        "tabs.connection" => "连接",
        "tabs.chat" => "聊天",
        "tabs.dashboard" => "概览",
        "tabs.system" => "系统",
        "tabs.channels" => "通道",
        "tabs.speech" => "语音",
        "tabs.keys" => "LLM提供商",
        "tabs.comm" => "通信",

        "speech.openai_tts" => "OpenAI 语音合成",
        "speech.local_model" => "本地语音模型",
        "speech.model" => "模型 ID",
        "speech.voice" => "预设音色",
        "speech.path" => "模型路径",
        "speech.enabled" => "启用",

        // Skills Sub-tabs
        "skills.installed" => "已安装",
        "skills.market" => "商店",
        "skills.manual" => "手动执行",

        // Manual Install
        "install.title" => "手动安装",
        "install.hint" => "访问 skills.sh 或 clawhub.ai 找到技能后，复制安装命令粘贴到下方。",
        "install.subtitle" => "安装技能",
        "install.paste_hint" => "粘贴来自 skills.sh 或 clawhub.ai 的安装命令，或 GitHub URL：",

        // Common Buttons
        "btn.refresh" => "刷新",
        "btn.clear" => "清空",
        "btn.save" => "保存",
        "btn.cancel" => "取消",
        "btn.add" => "添加",
        "btn.apply" => "应用",
        "btn.run" => "运行",
        "btn.kill" => "停止",

        // Dashboard Metrics
        "dashboard.token_usage_title" => "Token使用与活动",
        "dashboard.total_tokens" => "总 Token 消耗",
        "dashboard.prompt_tokens" => "提示词 Token",
        "dashboard.completion_tokens" => "完成词 Token",
        "dashboard.total_calls" => "总调用次数",
        "dashboard.avg_latency" => "平均延迟",
        "dashboard.call_volume" => "调用量趋势",

        // System Doctor
        "system.diagnostics" => "系统诊断",
        "system.run_doctor" => "一键健康检查",
        "system.status_wall" => "状态墙",
        "system.emergency_brake" => "🛑 紧急停止",
        "system.sandboxes" => "沙箱管理",
        "system.kill_pid" => "终止进程",
        "system.running_sandboxes" => "运行中的沙箱",

        // Blueprints
        "blueprint.gallery" => "灵魂蓝图",
        "blueprint.apply" => "应用模板",
        "blueprint.category" => "分类",
        "btn.new_agent" => "新建 Agent",

        // Connection
        "conn.gateway_url" => "网关地址",
        "conn.connected" => "已连接",
        "conn.disconnected" => "未连接",
        "conn.connecting" => "正在连接...",
        "misc.connected" => "已连接",
        "misc.disconnected" => "未连接",

        // Misc
        "misc.theme" => "主题",
        "misc.language" => "语言",
        "misc.searching" => "查询中...",
        "misc.no_data" => "暂无数据",
        "soul.export" => "导出灵核",
        "soul.memory_depth" => "记忆深度",
        "soul.export_success" => "灵核导出成功！",
        "soul.export_failed" => "导出失败",

        _ => key,
    }
}
