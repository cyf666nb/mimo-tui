/// First-run setup wizard — interactive onboarding like Claude Code.
use crate::config::Config;
use std::io::{self, Write};

const BANNER: &str = r#"
  ╔══════════════════════════════════════════════════╗
  ║                                                  ║
  ║   ███╗   ███╗ ██╗ ███╗   ███╗  ██████╗          ║
  ║   ████╗ ████║ ██║ ████╗ ████║ ██╔═══██╗         ║
  ║   ██╔████╔██║ ██║ ██╔████╔██║ ██║   ██║         ║
  ║   ██║╚██╔╝██║ ██║ ██║╚██╔╝██║ ██║   ██║         ║
  ║   ██║ ╚═╝ ██║ ██║ ██║ ╚═╝ ██║ ╚██████╔╝         ║
  ║   ╚═╝     ╚═╝ ╚═╝ ╚═╝     ╚═╝  ╚═════╝          ║
  ║                                                  ║
  ║   Terminal Coding Agent                     v0.2 ║
  ╚══════════════════════════════════════════════════╝
"#;

const PROVIDERS: &[(&str, &str, &str, &str)] = &[
    // (name, base_url, default_model, description)
    (
        "MiMo (TokenPlan)",
        "https://token-plan-cn.xiaomimimo.com/v1",
        "mimo-v2.5-pro",
        "Xiaomi MiMo — best value, Chinese-friendly",
    ),
    (
        "MiMo (Official)",
        "https://api.xiaomimimo.com/v1",
        "mimo-v2.5-pro",
        "Xiaomi MiMo official API",
    ),
    (
        "OpenAI",
        "https://api.openai.com/v1",
        "gpt-4o",
        "GPT-4o, o1, o3 — industry standard",
    ),
    (
        "DeepSeek",
        "https://api.deepseek.com/v1",
        "deepseek-chat",
        "DeepSeek V3/R1 — strong reasoning",
    ),
    (
        "OpenRouter",
        "https://openrouter.ai/api/v1",
        "anthropic/claude-sonnet-4",
        "Multi-model gateway — 200+ models",
    ),
    (
        "Custom",
        "",
        "",
        "Any OpenAI-compatible endpoint",
    ),
];

/// Check if first-run setup is needed
pub fn needs_setup() -> bool {
    !Config::config_file().exists()
}

/// Run the interactive setup wizard
pub fn run_setup() -> Config {
    print!("\x1b[2J\x1b[H"); // Clear screen
    println!("{}", BANNER);
    println!("  Welcome! Let's set up your AI coding agent.\n");

    // Step 1: Choose provider
    println!("  ┌─ Choose your AI provider ─────────────────────┐");
    for (i, (name, _, _, desc)) in PROVIDERS.iter().enumerate() {
        println!("  │  {}. {:<20} {}",
            i + 1, name, desc);
    }
    println!("  └───────────────────────────────────────────────┘\n");

    let choice = prompt_choice("  Provider [1-6]", 1, PROVIDERS.len());
    let (provider_name, default_url, default_model, _) = PROVIDERS[choice - 1];

    println!();

    // Step 2: API key
    let api_key = if provider_name == "Custom" || !default_url.is_empty() {
        prompt_secret(&format!("  API key for {}", provider_name))
    } else {
        prompt_secret("  API key")
    };

    if api_key.is_empty() {
        eprintln!("\n  Error: API key cannot be empty.");
        std::process::exit(1);
    }

    println!();

    // Step 3: Base URL
    let base_url = if provider_name == "Custom" {
        prompt("  Base URL (e.g. https://api.example.com/v1)", "")
    } else if default_url.is_empty() {
        prompt("  Base URL", "")
    } else {
        let use_default = prompt_yes_no(
            &format!("  Base URL [{}]", default_url), true);
        if use_default {
            default_url.to_string()
        } else {
            prompt("  Base URL", default_url)
        }
    };

    println!();

    // Step 4: Model
    let model = if provider_name == "Custom" {
        prompt("  Model name", "")
    } else {
        let use_default = prompt_yes_no(
            &format!("  Model [{}]", default_model), true);
        if use_default {
            default_model.to_string()
        } else {
            prompt("  Model", default_model)
        }
    };

    println!();

    // Step 5: Extras
    println!("  ┌─ Quick settings (can change later with /config) ─┐");
    let thinking = prompt_yes_no("  Enable deep thinking? [Y/n]", true);
    let web_search = prompt_yes_no("  Enable web search? [y/N]", false);
    println!("  └───────────────────────────────────────────────────┘\n");

    // Build config
    let config = Config {
        api_key,
        base_url,
        model,
        thinking,
        permission_mode: "agent".into(),
        max_output_tokens: 16384,
        web_search,
        auto_routing: false,
        system_prompt_extra: String::new(),
    };

    // Step 6: Test connection
    print!("  Testing connection... ");
    io::stdout().flush().ok();
    match test_connection(&config) {
        Ok(model_info) => {
            println!("✓ Connected! ({})", model_info);
        }
        Err(e) => {
            println!("⚠ Could not verify: {}", e);
            println!("  Saving anyway — you can fix settings with /config\n");
        }
    }

    // Save
    config.save();
    println!("\n  ✓ Config saved to ~/.mimo-tui/config.toml");
    println!("  ✓ Ready to code! Launching...\n");

    config
}

/// Test API connection with a minimal request
fn test_connection(config: &Config) -> Result<String, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let url = format!("{}/models", config.base_url.trim_end_matches('/'));
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", config.api_key))
        .send()
        .map_err(|e| format!("Network error: {}", e))?;

    if resp.status().is_success() {
        // Try to extract model count
        if let Ok(body) = resp.json::<serde_json::Value>() {
            if let Some(data) = body.get("data").and_then(|d| d.as_array()) {
                return Ok(format!("{} models available", data.len()));
            }
        }
        Ok("API reachable".to_string())
    } else {
        Err(format!("HTTP {}", resp.status()))
    }
}

// ── Interactive prompts ──

fn prompt(message: &str, default: &str) -> String {
    print!("{}: ", message);
    io::stdout().flush().ok();

    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap_or(0);
    let input = input.trim().to_string();

    if input.is_empty() && !default.is_empty() {
        default.to_string()
    } else {
        input
    }
}

fn prompt_secret(message: &str) -> String {
    print!("{}: ", message);
    io::stdout().flush().ok();

    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap_or(0);
    input.trim().to_string()
}

fn prompt_choice(message: &str, min: usize, max: usize) -> usize {
    loop {
        print!("{}: ", message);
        io::stdout().flush().ok();

        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap_or(0);
        let input = input.trim();

        if let Ok(n) = input.parse::<usize>() {
            if n >= min && n <= max {
                return n;
            }
        }
        println!("  Please enter a number between {} and {}", min, max);
    }
}

fn prompt_yes_no(message: &str, default_yes: bool) -> bool {
    loop {
        print!("{}: ", message);
        io::stdout().flush().ok();

        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap_or(0);
        let input = input.trim().to_lowercase();

        if input.is_empty() {
            return default_yes;
        }
        if input == "y" || input == "yes" {
            return true;
        }
        if input == "n" || input == "no" {
            return false;
        }
        println!("  Please enter y or n");
    }
}
