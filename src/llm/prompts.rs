use crate::tools::ToolRegistry;

/// Build the system prompt appended to every LLM invocation.
///
/// When `tools` is provided, appends the tool-calling protocol and
/// per-tool JSON schemas so the LLM can propose structured tool calls.
///
/// `timezone` is the IANA timezone name for the user (e.g. "America/New_York").
/// When provided, the current local time in that timezone is injected into the
/// prompt so the LLM can give time-aware responses.
///
/// `locale` is a BCP 47 locale tag (e.g. "en-US", "ja-JP"). When provided and
/// not English, instructs the LLM to respond in the user's preferred language.
pub fn system_prompt(
    personality: &str,
    agent_name: &str,
    tools: Option<&ToolRegistry>,
    timezone: Option<&str>,
    locale: Option<&str>,
    prompt_skills: &[crate::skills::PromptSkill],
) -> String {
    let base = if personality.is_empty() {
        format!("You are {agent_name}, a helpful AI assistant.")
    } else {
        personality.to_string()
    };

    let tool_section = match tools {
        Some(registry) if !registry.is_empty() => build_tool_section(registry),
        _ => String::new(),
    };

    let time_section = build_time_section(timezone);
    let locale_section = build_locale_section(locale);
    let skills_section = build_prompt_skills_section(prompt_skills);

    format!(
        r#"{base}

You are communicating with the user via Telegram.
Keep replies concise and conversational.
Do not use markdown formatting unless the user asks for it.
{time_section}
{locale_section}
{tool_section}
{skills_section}== SKILL SYSTEM ==

You can create persistent services ("skills") that run alongside you.
Skills are Python scripts managed by the agent's skill manager.

To create a skill:

1. Create a directory under /data/safeclaw/skills/<skill-name>/
2. Write the Python script as main.py (or whatever entrypoint you choose)
3. Create a skill.toml manifest in the same directory
4. Optionally create requirements.txt for extra pip packages

The skill manager automatically discovers new skills and starts them.
If a daemon skill crashes, it will be restarted on the next tick (~2 min).

== skill.toml format ==

name = "my-skill"
description = "What this skill does"
skill_type = "daemon"    # "daemon" (long-running) or "oneshot" (run once)
enabled = true
entrypoint = "main.py"   # default

[env]
# Extra environment variables (optional, non-secret)
SOME_KEY = "value"

# Declare credentials the skill needs. The operator configures actual
# values in the web dashboard — they are injected as env vars at runtime.
[[credentials]]
name = "GOOGLE_API_KEY"          # env var name passed to the skill
label = "Google API Key"         # human-readable label shown in dashboard
description = "API key for Google Calendar access"
required = true

[[credentials]]
name = "WEBHOOK_SECRET"
label = "Webhook Secret"
description = "Optional webhook signing secret"
required = false

== Environment variables available to skills ==

TELEGRAM_BOT_TOKEN  - Bot token for sending Telegram messages
TELEGRAM_CHAT_ID    - Chat ID to send messages to
SKILL_NAME          - Name of this skill
SKILL_DIR           - Path to this skill's directory
SKILL_DATA_DIR      - Path to this skill's persistent data directory

== Sending Telegram messages from a skill ==

Use the Telegram Bot API directly via HTTP:

```python
import os, requests

TOKEN = os.environ["TELEGRAM_BOT_TOKEN"]
CHAT_ID = os.environ["TELEGRAM_CHAT_ID"]

def send_message(text):
    requests.post(
        f"https://api.telegram.org/bot{{TOKEN}}/sendMessage",
        json={{"chat_id": CHAT_ID, "text": text}}
    )
```

== CONNECTED OAUTH ACCOUNTS ==

The user has connected external service accounts through the dashboard's
OAuth system.  A discovery manifest listing every connected account, its
provider, scopes, and capabilities lives at:

  /data/safeclaw/oauth/manifest.json

Read that file FIRST whenever the user asks you to interact with an
external service (calendar, email, files, repos, messaging, etc.).
It looks like:

```json
{{
  "accounts": [
    {{
      "provider": "google",
      "account": "user@gmail.com",
      "scopes": "...calendar ...gmail.readonly ...",
      "capabilities": ["calendar", "email", "files"],
      "token_file": "/data/safeclaw/oauth/google/user@gmail.com.json"
    }},
    {{
      "provider": "microsoft",
      "account": "user@outlook.com",
      "scopes": "Calendars.Read Mail.Read ...",
      "capabilities": ["calendar", "email"],
      "token_file": "/data/safeclaw/oauth/microsoft/user@outlook.com.json"
    }}
  ]
}}
```

Each token file contains: provider, account, access_token, refresh_token,
client_id, client_secret, token_url, and scopes.

IMPORTANT RULES:
- ALWAYS read the manifest before accessing any external service.
- NEVER ask the user to create new OAuth credentials or go to a cloud
  console.  The tokens ALREADY EXIST.  Use them via exec + Python.
- NEVER create a new skill with its own OAuth flow for a service that
  is already represented in the manifest.
- If the manifest is missing or empty, tell the user they need to
  connect the relevant account in the dashboard Settings > OAuth tab.

== How to use tokens by provider ==

Google (calendar, gmail, drive):
```python
from google.oauth2.credentials import Credentials
from google.auth.transport.requests import Request
from googleapiclient.discovery import build
creds = Credentials.from_authorized_user_file("<token_file>")
if creds.expired and creds.refresh_token:
    creds.refresh(Request())
service = build("calendar", "v3", credentials=creds)
```
Note: for Google, legacy authorized_user files also exist at
/data/safeclaw/skills/google-oauth/data/accounts/ and
/data/safeclaw/skills/calendar-reminder/data/credentials/.

Microsoft (outlook calendar, mail, onedrive):
```python
import json, requests
token = json.load(open("<token_file>"))
headers = {{"Authorization": f"Bearer {{token['access_token']}}"}}
r = requests.get("https://graph.microsoft.com/v1.0/me/calendarview"
                  "?startDateTime=...&endDateTime=...", headers=headers)
events = r.json().get("value", [])
```

GitHub:
```python
import json, requests
token = json.load(open("<token_file>"))
headers = {{"Authorization": f"token {{token['access_token']}}"}}
repos = requests.get("https://api.github.com/user/repos", headers=headers).json()
```

Other providers (Slack, Discord, Spotify, Dropbox, LinkedIn, Notion, Twitter):
Load token_file, use access_token as a Bearer token with their REST API.
If a token is expired, refresh it by POSTing to the token_url with the
refresh_token, client_id, and client_secret from the token file.

A calendar-reminder daemon skill is already running that sends Telegram
alerts 10 minutes before events for all linked calendar accounts.

== Important guidelines ==

- Skills run as background processes inside a Docker container
- The container has Python 3, pip, and common packages pre-installed:
  requests, google-api-python-client, google-auth-httplib2,
  google-auth-oauthlib, schedule, httpx, beautifulsoup4, feedparser,
  icalendar
- For additional packages, add them to requirements.txt
- Store persistent data in SKILL_DATA_DIR
- Log output goes to skill.log in the skill directory
- Daemon skills should run in an infinite loop with appropriate sleep
- Always include error handling and graceful degradation
- When the user asks you to create a capability or service, create it as a skill
- After creating the skill files, tell the user it will start automatically
- If the user provides credentials or tokens, add [[credentials]] entries to
  skill.toml so the operator can configure them via the dashboard. Never
  hardcode secrets. The dashboard at /api/skills shows credential status.
- When the user asks about ANY external service (calendar, email, repos, etc.),
  read /data/safeclaw/oauth/manifest.json and use the existing OAuth tokens
  via exec + Python.  Do NOT create a new skill with its own credentials flow.
"#
    )
}

/// Build a short section telling the LLM the current date/time in the user's
/// timezone so it can give time-aware responses (greetings, scheduling, etc.).
fn build_time_section(timezone: Option<&str>) -> String {
    use chrono::Utc;

    let tz_name = timezone.unwrap_or("UTC");
    let tz: chrono_tz::Tz = tz_name.parse().unwrap_or(chrono_tz::UTC);
    let now = Utc::now().with_timezone(&tz);
    let formatted = now.format("%A, %B %-d, %Y at %-I:%M %p %Z").to_string();

    format!(
        "The current date and time for the user is: {formatted} ({tz_name}).\n\
         Use this when the user asks about the time, scheduling, or when context \
         requires awareness of the current date or time of day."
    )
}

fn build_locale_section(locale: Option<&str>) -> String {
    let code = match locale {
        Some(l) if !l.is_empty() => l,
        _ => return String::new(),
    };

    let lang = match code.split('-').next().unwrap_or(code) {
        "en" => return String::new(),
        "es" => "Spanish",
        "fr" => "French",
        "de" => "German",
        "ja" => "Japanese",
        "zh" => "Chinese (Simplified)",
        "pt" => "Portuguese (Brazilian)",
        "ko" => "Korean",
        "it" => "Italian",
        "ru" => "Russian",
        "ar" => "Arabic",
        "hi" => "Hindi",
        other => other,
    };

    format!(
        "The user's preferred language is {lang} ({code}). \
         Always respond in {lang} unless the user explicitly writes in or asks for a different language.\n"
    )
}

/// Build the section that injects loaded prompt-skill bodies into the system
/// prompt.  Returns an empty string when no skills are loaded so the prompt
/// stays clean for the default (no-plugin) case.
///
/// When a skill has reference files (loaded from `references/*.md`), they
/// are appended after the skill body under sorted sub-headings for
/// deterministic output.
fn build_prompt_skills_section(skills: &[crate::skills::PromptSkill]) -> String {
    if skills.is_empty() {
        return String::new();
    }
    let mut section = String::from("\n== LOADED SKILLS ==\n\n");
    for skill in skills {
        section.push_str(&format!("### {}\n", skill.name));
        if !skill.description.is_empty() {
            section.push_str(&format!("{}\n\n", skill.description));
        }
        section.push_str(&skill.body);
        section.push('\n');

        if !skill.references.is_empty() {
            let mut refs: Vec<(&String, &String)> = skill.references.iter().collect();
            refs.sort_by_key(|(name, _)| *name);
            section.push_str("\n#### References\n\n");
            for (filename, content) in refs {
                section.push_str(&format!("##### {filename}\n\n"));
                section.push_str(content);
                section.push_str("\n\n");
            }
        }

        section.push('\n');
    }
    section
}

/// Build the tool-calling protocol section with per-tool schemas.
fn build_tool_section(registry: &ToolRegistry) -> String {
    let mut tools: Vec<_> = registry.list().iter().map(|(n, _)| n.to_string()).collect();
    tools.sort();

    let mut schemas = String::new();
    for name in &tools {
        if let Some(tool) = registry.get(name) {
            let schema = tool.parameters_schema();
            let schema_str = serde_json::to_string(&schema).unwrap_or_default();
            schemas.push_str(&format!(
                "### {name}\n{desc}\nParameters: {schema_str}\n\n",
                name = tool.name(),
                desc = tool.description(),
            ));
        }
    }

    format!(
        r#"
== TOOL CALLING ==

You have tools you can use to take actions. To call a tool, emit a
fenced code block tagged "tool_call" containing a JSON object:

```tool_call
{{"tool": "tool_name", "params": {{}}, "reasoning": "brief explanation"}}
```

Rules:
- "tool" must be one of the tool names listed below.
- "params" is an object matching the tool's parameter schema.
- "reasoning" is a short explanation of why you are calling this tool.
- You may include MULTIPLE tool_call blocks in one response.
- You may include natural-language text before, between, and after
  tool_call blocks to explain your thinking.
- After the tools execute, you will see the results and should give
  the user a final natural-language answer.
- If you do NOT need a tool, just reply with normal text (no blocks).
- Prefer using tools over telling the user to do something themselves.

== AVAILABLE TOOLS ==

{schemas}"#,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::{Tool, ToolOutput, ToolRegistry, ToolContext};
    use async_trait::async_trait;

    struct MockPromptTool {
        name: &'static str,
        description: &'static str,
    }

    #[async_trait]
    impl Tool for MockPromptTool {
        fn name(&self) -> &str {
            self.name
        }

        fn description(&self) -> &str {
            self.description
        }

        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object"})
        }

        async fn execute(
            &self,
            _params: serde_json::Value,
            _ctx: &ToolContext,
        ) -> crate::error::Result<ToolOutput> {
            Ok(ToolOutput::ok("ok"))
        }
    }

    fn registry_with_mock_tool() -> ToolRegistry {
        let mut reg = ToolRegistry::new();
        reg.register(Box::new(MockPromptTool {
            name: "test_tool",
            description: "A tool for testing",
        }));
        reg
    }

    #[test]
    fn test_system_prompt_empty_personality() {
        let prompt = system_prompt("", "TestAgent", None, None, None, &[]);
        assert!(prompt.contains("You are TestAgent, a helpful AI assistant."));
        assert!(!prompt.contains("== AVAILABLE TOOLS =="));
    }

    #[test]
    fn test_system_prompt_with_personality() {
        let personality = "You are a specialized coding assistant.";
        let prompt = system_prompt(personality, "TestAgent", None, None, None, &[]);
        assert!(prompt.contains("You are a specialized coding assistant."));
        assert!(!prompt.contains("You are TestAgent, a helpful AI assistant."));
    }

    #[test]
    fn test_system_prompt_none_tools() {
        let prompt = system_prompt("", "Agent", None, None, None, &[]);
        assert!(!prompt.contains("== AVAILABLE TOOLS =="));
        assert!(!prompt.contains("== TOOL CALLING =="));
    }

    #[test]
    fn test_system_prompt_empty_registry() {
        let reg = ToolRegistry::new();
        let prompt = system_prompt("", "Agent", Some(&reg), None, None, &[]);
        assert!(!prompt.contains("test_tool"));
        assert!(prompt.contains("== SKILL SYSTEM =="));
    }

    #[test]
    fn test_system_prompt_with_registry_containing_tool() {
        let reg = registry_with_mock_tool();
        let prompt = system_prompt("", "Agent", Some(&reg), None, None, &[]);

        assert!(prompt.contains("== AVAILABLE TOOLS =="));
        assert!(prompt.contains("test_tool"));
        assert!(prompt.contains("A tool for testing"));
        assert!(prompt.contains("== TOOL CALLING =="));
        assert!(prompt.contains("Parameters:"));
    }

    #[test]
    fn test_build_tool_section_indirect() {
        let reg = registry_with_mock_tool();
        let prompt = system_prompt("", "Agent", Some(&reg), None, None, &[]);

        assert!(prompt.contains("### test_tool"));
        assert!(prompt.contains("A tool for testing"));
        assert!(prompt.contains("object"));
    }

    #[test]
    fn test_system_prompt_includes_timezone() {
        let prompt = system_prompt("", "Agent", None, Some("America/New_York"), None, &[]);
        assert!(prompt.contains("America/New_York"));
        assert!(prompt.contains("current date and time"));
    }

    #[test]
    fn test_system_prompt_utc_fallback() {
        let prompt = system_prompt("", "Agent", None, Some("UTC"), None, &[]);
        assert!(prompt.contains("UTC"));
    }

    #[test]
    fn test_system_prompt_with_prompt_skills() {
        use crate::skills::PromptSkill;
        use std::collections::HashMap;

        let skills = vec![PromptSkill {
            name: "test-skill".into(),
            description: "A test skill".into(),
            enabled: true,
            triggers: vec![],
            body: "Always be helpful and concise.".into(),
            references: HashMap::new(),
        }];

        let prompt = system_prompt("", "Agent", None, None, None, &skills);
        assert!(prompt.contains("== LOADED SKILLS =="));
        assert!(prompt.contains("### test-skill"));
        assert!(prompt.contains("Always be helpful and concise."));
    }

    #[test]
    fn test_system_prompt_no_skills_section_when_empty() {
        let prompt = system_prompt("", "Agent", None, None, None, &[]);
        assert!(!prompt.contains("== LOADED SKILLS =="));
    }

    #[test]
    fn test_prompt_skill_references_injected_sorted() {
        use crate::skills::PromptSkill;
        use std::collections::HashMap;

        let mut refs = HashMap::new();
        refs.insert("z-style.md".into(), "Use snake_case everywhere.".into());
        refs.insert("a-rules.md".into(), "No globals allowed.".into());

        let skills = vec![PromptSkill {
            name: "ref-skill".into(),
            description: "Skill with references".into(),
            enabled: true,
            triggers: vec![],
            body: "Follow the attached references.".into(),
            references: refs,
        }];

        let prompt = system_prompt("", "Agent", None, None, None, &skills);
        assert!(prompt.contains("#### References"));
        assert!(prompt.contains("##### a-rules.md"));
        assert!(prompt.contains("No globals allowed."));
        assert!(prompt.contains("##### z-style.md"));
        assert!(prompt.contains("Use snake_case everywhere."));

        // Verify alphabetical ordering: a-rules.md should appear before z-style.md
        let a_pos = prompt.find("##### a-rules.md").unwrap();
        let z_pos = prompt.find("##### z-style.md").unwrap();
        assert!(a_pos < z_pos, "references should be sorted alphabetically");
    }
}
