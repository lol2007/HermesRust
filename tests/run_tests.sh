#!/usr/bin/env bash
# HermesRust functional test suite — run against a local build + mock upstream.
# Usage: bash tests/run_tests.sh     (expects app on :8642 pointed at the mock on :9999)
set -u
BASE="http://127.0.0.1:8642"
PASS=0; FAIL=0
ok()   { PASS=$((PASS+1)); echo "  ✓ $1"; }
fail() { FAIL=$((FAIL+1)); echo "  ✗ $1 — got: ${2:0:200}"; }
check() {
  if echo "$3" | grep -q "$2"; then ok "$1"; else fail "$1" "$3"; fi
}

echo "── 1. health"
R=$(curl -s $BASE/health); check "GET /health" '"status":"ok"' "$R"; check "platform" 'hermes-rust' "$R"

echo "── 2. static UI served from disk"
R=$(curl -s $BASE/ | head -8); check "GET / serves index.html" 'HermesRust' "$R"
R=$(curl -s -o /dev/null -w "%{http_code} %{content_type}" $BASE/app.css); check "GET /app.css" 'text/css' "$R"
R=$(curl -s -o /dev/null -w "%{http_code} %{content_type}" $BASE/app.js); check "GET /app.js" 'javascript' "$R"

echo "── 3. models"
R=$(curl -s $BASE/v1/models); check "GET /v1/models lists config models" 'gpt-4o-mini' "$R"

echo "── 4. agents & skills"
R=$(curl -s $BASE/api/agents); check "GET /api/agents" 'agent-pm' "$R"
N=$(curl -s $BASE/api/agents | python3 -c "import json,sys;print(json.load(sys.stdin)['count'])")
check "20 agents loaded" '^20$' "$N"
R=$(curl -s $BASE/api/skills); check "GET /api/skills" 'tavily-search-pro' "$R"

echo "── 5. folders"
R=$(curl -s "$BASE/api/list-folder?path=~"); check "list-folder home" '"dirs"' "$R"
R=$(curl -s "$BASE/api/list-folder?path=/definitely/not/here"); check "list-folder bad path" 'error' "$R"

echo "── 6. streaming chat via proxy (SSE re-framing + orchestrator injection)"
R=$(curl -s -N -X POST $BASE/v1/chat/completions -H "Content-Type: application/json" \
  -d "{\"model\":\"mock-large\",\"stream\":true,\"messages\":[{\"role\":\"user\",\"content\":\"hi\"}]}")
check "SSE role chunk" '"role": "assistant"' "$R"
check "SSE content streamed" 'mock team' "$R"
check "SSE [DONE]" '\[DONE\]' "$R"
python3 - <<'EOF'
import json, os
p = os.path.join(os.path.dirname(__file__), '..', 'mock_upstream_last.json')
d = json.load(open(p))
msgs = d["body"]["messages"]
has_system = bool(msgs) and msgs[0]["role"] == "system"
print("  ✓ orchestrator system prompt injected upstream" if has_system else "  ✗ NO system prompt injected")
print("  ✓ upstream auth forwarded" if d["auth"] == "Bearer mock-key-test" else f"  ✗ auth wrong: {d['auth']}")
print("  ✓ hermes meta stripped from upstream body" if "hermes" not in d["body"] else "  ✗ hermes meta leaked upstream")
print("  ✓ upstream model = mock-large" if d["body"].get("model") == "mock-large" else "  ✗ model wrong")
if has_system:
    p2 = msgs[0]["content"]
    print("  ✓ system prompt contains team roster" if "Team members" in p2 and "Frontend Engineer" in p2 else "  ✗ roster missing")
EOF

echo "── 7. non-streaming chat"
R=$(curl -s -X POST $BASE/v1/chat/completions -H "Content-Type: application/json" \
  -d '{"model":"mock-large","stream":false,"messages":[{"role":"user","content":"hi"}]}')
check "non-stream JSON" '"content": "mock ok"' "$R"

echo "── 8. config hot-reload (edit file, no restart)"
sed -i.bak 's/^model = .*/model = "hot-reloaded-model"/' config/config.toml
sleep 1.2
R=$(curl -s $BASE/api/config)
check "config.toml reloaded live" 'hot-reloaded-model' "$R"

echo "── 9. PATCH /api/config (live save from UI)"
R=$(curl -s -X PATCH $BASE/api/config -H "Content-Type: application/json" -d '{"model":"patched-model"}')
check "PATCH ok" '"ok":true' "$R"
R=$(curl -s $BASE/api/config); check "PATCH reflected" 'patched-model' "$R"
C=$(curl -s $BASE/api/config | grep -c "patched-key" || true)
check "api key never returned by GET" '^0$' "$C"

echo "── 10. agents.json hot-reload"
python3 - <<'EOF'
import json
a = json.load(open('config/agents.json'))
a.append({"id":"agent-test","name":"Test Agent","abbr":"TA","description":"live test","icon":"🧪","color":"#123456","skills":["x"],"isMain":False,"isBuiltin":False})
json.dump(a, open('config/agents.json','w'), indent=2, ensure_ascii=False)
EOF
sleep 1.2
R=$(curl -s $BASE/api/agents); check "agents.json reloaded live" 'Test Agent' "$R"

echo "── 11. orchestrator=false disables injection (via live PATCH)"
curl -s -X PATCH $BASE/api/config -H "Content-Type: application/json" -d '{"orchestrator":false}' > /dev/null
sleep 0.8
curl -s -X POST $BASE/v1/chat/completions -H "Content-Type: application/json" -d '{"stream":false,"messages":[{"role":"user","content":"hi"}]}' > /dev/null
python3 - <<'EOF'
import json, os
d = json.load(open(os.path.join(os.path.dirname(__file__), '..', 'mock_upstream_last.json')))
ok = d["body"]["messages"][0]["role"] != "system"
print("  ✓ no system prompt when orchestrator=false" if ok else "  ✗ injection still on")
EOF
curl -s -X PATCH $BASE/api/config -H "Content-Type: application/json" -d '{"orchestrator":true}' > /dev/null

echo "── 12. restore config + agents"
python3 - <<'EOF'
s = open('config/config.toml').read().replace('model = "patched-model"', 'model = "gpt-4o-mini"')
open('config/config.toml', 'w').write(s)
import json, os
if os.path.exists('config/config.toml.bak'): os.remove('config/config.toml.bak')
a = json.load(open('config/agents.json'))
a = [x for x in a if x["id"] != "agent-test"]
json.dump(a, open('config/agents.json','w'), indent=2, ensure_ascii=False)
EOF

echo ""
echo "═══ RESULTS: $PASS passed, $FAIL failed ═══"
[ $FAIL -eq 0 ]
