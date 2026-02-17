// Claude Monitor extension routes

__routes.push(register_route("GET", "/status", "handle_status"));

fn read_state(data_dir) {
    let path = data_dir + "/state.json";
    if !data_exists(path) { return #{}; }
    let raw = data_read(path);
    if raw == () { return #{}; }
    let parsed = json_parse(raw);
    if parsed == () { return #{}; }
    parsed
}

fn read_token_info() {
    let paths = ["/home/agent/.claude/.credentials.json"];
    let info = #{ expires_in_secs: 0, healthy: false };
    for path in paths {
        if !data_exists(path) { continue; }
        let raw = data_read(path);
        if raw == () { continue; }
        let creds = json_parse(raw);
        if creds == () { continue; }
        let oauth = creds.claudeAiOauth;
        if oauth == () { continue; }
        let exp_ms = oauth.expiresAt;
        let remaining = (exp_ms / 1000) - now_epoch();
        info.expires_in_secs = remaining;
        info.healthy = remaining > 600;
        break;
    }
    info
}

fn read_usage() {
    let paths = ["/home/agent/.claude/stats-cache.json"];
    let usage = #{ today_tokens: 0, today_messages: 0, total_messages: 0, total_sessions: 0 };
    for path in paths {
        if !data_exists(path) { continue; }
        let raw = data_read(path);
        if raw == () { continue; }
        let stats = json_parse(raw);
        if stats == () { continue; }
        usage.total_messages = stats.totalMessages;
        usage.total_sessions = stats.totalSessions;
        let today_date = now_utc().sub_string(0, 10);
        let daily = stats.dailyActivity;
        if daily != () {
            for day in daily {
                if day.date == today_date { usage.today_messages = day.messageCount; }
            }
        }
        let tokens = stats.dailyModelTokens;
        if tokens != () {
            for day in tokens {
                if day.date == today_date {
                    let by_model = day.tokensByModel;
                    if by_model != () {
                        for m in by_model.keys() { usage.today_tokens += by_model[m]; }
                    }
                }
            }
        }
        break;
    }
    usage
}

fn handle_status(req) {
    let state = read_state(__data_dir);
    let token = read_token_info();
    let usage = read_usage();
    let budget = 45000;
    let pct = 0;
    if budget > 0 { pct = (usage.today_tokens * 100) / budget; }

    json_response(#{
        token: token,
        usage: usage,
        usage_percent: pct,
        daily_budget: budget,
        state: state,
        checked_at: now_utc()
    })
}
