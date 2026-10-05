# FNDR positioning

Status: draft for PD-02, 2026-10-04. Competitor facts checked on the web on 2026-10-04 (sources at the end). FNDR facts come from the repo and its evidence files. Re-check prices and products before any slide; they move.

## Who it is for

Knowledge workers who do school, office, and project work on a MacBook, switch between many apps, get interrupted, and already use an AI assistant (ChatGPT, Claude, Cursor). Month plan section 2 has the three people we design for: a student, an analyst or PM, and a project lead.

Not yet: people on Windows or Linux, teams sharing one memory, phone users.

## The problem, in their words

These lines are our hypothesis. Replace them with real quotes from PD-18 and PD-13 as soon as those conversations happen.

- "I know I read it this week. I just can't remember where, or what it was called."
- "After a meeting it takes me ten minutes to figure out where I was."
- "Every time I open ChatGPT or Claude I have to explain my project again."
- "I'd like an AI that remembers my screen, but I don't want my screen on someone's server."

## FNDR in one sentence

> FNDR is the work memory for your Mac: find anything you saw by what it meant, get back to it, and give your AI assistant the context, while your captures stay on your Mac.

The longer version is month plan section 1. Do not add "everything that leaves your Mac is visible to you" to slides yet: Privacy Activity does not count every cloud path today (`docs/decisions/018-reasoning-tier.md`, "What is true today").

## Three proof points we can show today

| # | Claim we can make | Evidence | What we must not overstate |
|---|---|---|---|
| 1 | Search finds work by meaning, not only exact words. On a seeded week of 20 synthetic memories, Search puts an accepted memory in the top five for 21 of 22 queries (Recall@5 0.955; paraphrase queries 0.875) and Ask for 22 of 22. | `docs/evidence/W02/VS-01-baseline.md`, `retrieval-baseline-seeded.md` | A small synthetic set, not a real vault. Search and Ask agree on the top result for only 17 of 22. |
| 2 | FNDR keeps no screenshots. Privacy and blocklist checks run before pixels are read, and skipped frames are counted by reason, without app names. | ADR-004; `src-tauri/src/privacy_proof.rs` and its test `proof_reports_skips_by_reason_and_never_carries_content` | The native blocklist run in `docs/evidence/W03/privacy-activity-native.md` is a runbook, not a recorded result yet. Counters reset on quit. |
| 3 | Your assistant can pick up where you left off: an MCP client such as Claude Code calls `memory.resume_work` (token required) and gets cited threads. | `docs/evidence/W03/resume-work-mcp.md`; `resume-work-latency.md` (p95 17.27 ms over 200 synthetic records); ADR-017 and its tests | Tested at the MCP boundary with fixtures; no live Claude Code session is recorded in evidence yet. Threads group by project, then session, domain, or app, and project is 0% filled on the owner's vault, so real threads are coarse today (`VS-01-baseline.md`). |

## What we are not

- Not a screen recorder or video archive. We keep text, metadata, and vectors, not pixels (ADR-004).
- Not a cloud service. There is no FNDR account and no FNDR server.
- Not an agent that acts for you without asking. The command bar never sends, deletes, or buys this semester (ADR-022).
- Not a phone app, a team memory, or life logging (month plan section 2).
- Not cross-platform. macOS only.

## Competitor teardown

[V] marks a vendor's own claim we have not verified. Prices are from the vendor's pricing page unless noted.

| Product | What it does | Price (2026-10-04) | Local or cloud | Where FNDR wins | Where FNDR loses |
|---|---|---|---|---|---|
| Screenpipe | Records screen text (accessibility tree first, OCR as fallback) and audio; search; an MCP server for assistants; scheduled workflows. macOS, Windows, Linux. | Free tier; Basic $21/month or $250/year; Business $42/seat/month. Its About page lists $25 and $50, so prices are in flux. | Local by default: text, transcripts, and compressed screenshots in a local SQLite database [V]. Optional encrypted sync on paid plans [V]. Source-available, commercial license. | Keeps no screenshots. Apache-2.0 and free, with no paid tier. | Nearly everything else today: three operating systems, audio and meetings, accessibility-first capture already shipped (ours is VS-15 and VS-16, not built), and a shipping product with paid plans. It is the closest competitor. |
| Rewind, then Limitless | Rewind was a Mac app that recorded screen and audio for natural-language search. Limitless added a wearable pendant. | Not for sale. Meta acquired Limitless (announced Dec 5, 2025). Rewind recording stopped Dec 19, 2025; the Pendant is no longer sold; existing customers are supported through 2026. | Rewind stored recordings on the Mac (per third-party histories). | FNDR exists and is open source, so a memory built with it does not depend on one company's fate. | Rewind proved the idea and had polish and a known brand. Its exit shows how hard the business is. |
| Microsoft Recall | Takes snapshots of the screen on Windows, with semantic search and a timeline. | No separate price; a Windows 11 feature on Copilot+ PCs only (40 TOPS NPU and 16 GB RAM required). | Local. Opt-in. Snapshots encrypted and unlocked with Windows Hello; sensitive-info filter on by default [V]. In April 2026 a security researcher showed that malware running as the same user can extract decrypted snapshots after the person signs in to Recall; Microsoft says this is not a security boundary bypass. | Runs on Macs, including an 8 GB M1 (our reference machine). Keeps text, not screenshots, so there is less to steal. Hands context to assistants over MCP. | Built into the OS with hardware-backed encryption and a sign-in gate. We found no app-level encryption in FNDR's storage code (`src-tauri/src/storage/`); it relies on the macOS account and FileVault. |
| Raycast AI | A Mac launcher with Quick AI, AI chat, agents, AI commands, and extensions. Paid plans list Memory and Screen Awareness. | AI is not in the Free plan. Pro $10/month (500 AI credits), Pro+ $20, Max $50 (usage-based since Sep 10, 2026). | Cloud models by default (OpenAI, Anthropic, and others), or your own key, or a local model [V]. Data stored locally, chats synced encrypted if you choose [V]. | Remembers what you saw across apps over days, not only what you asked. Search over your own history. No cloud model call by default. | A fast, polished command bar that is shipping; ours is still being built (GS tickets). Extensions and frontier models. |
| ChatGPT desktop memory | Remembers saved facts and references past chats; a new memory summary rolled out from June 2026 (Plus and Pro in the US first). On the Mac, "Work with Apps" reads selected apps when you ask. | Free tier; Plus $20/month (third-party price list; OpenAI's page blocked our fetch). | Cloud. | Remembers your work outside the chat window, across every app, on your Mac. Any MCP client can use it, not only one vendor's. | Frontier models and zero setup: no screen permission, no local model download. |
| Claude desktop memory | Memory of your role, projects, and preferences from chats on every plan (on by default for Free, Pro, Max); chat search on paid plans; incognito chats. The desktop app connects to local MCP servers. | Free; Pro $20/month ($17 billed yearly); Max from $100/month. | Cloud. | Complementary more than competing: FNDR is an MCP server and Claude's apps are MCP clients, so FNDR can supply what you did outside Claude. It remembers screens, not chats. `docs/mcp.md` documents CLI clients such as Claude Code; we have not checked the Claude desktop app against FNDR's server. | Better models, no setup, and it runs on web, desktop, and mobile. If Claude's memory is good enough for a person, FNDR has to earn its screen permission. |

### Where FNDR loses to all of them today

- Thin memories: median stored text is 129 characters and the chunk table is empty on the owner's vault (`VS-01-baseline.md`).
- Reopen: 10.3% of memories reopen to the exact page, file, or deep link (`VS-01-baseline.md`; target 90%).
- Install: builds are ad-hoc signed, not notarized; first launch needs right-click, Open (`README.md`).
- No outside user session is recorded yet. PD-13 runs the first five.

## Sources

All accessed 2026-10-04.

| Fact | Source |
|---|---|
| Screenpipe plans and prices | https://screenpipe.com/pricing |
| Screenpipe capture, local storage with compressed screenshots, platforms, different prices on About page [V] | https://screenpipe.com/about |
| Screenpipe license, MCP server, accessibility tree first with OCR fallback [V] | https://github.com/screenpipe/screenpipe |
| Meta acquires Limitless; Rewind capture disabled Dec 19, 2025; Pendant no longer sold | https://9to5mac.com/2025/12/05/rewind-limitless-meta-acquisition/ |
| Limitless acquired by Meta; Pendant support through 2026 [V] | https://www.limitless.ai/ |
| Rewind stored recordings locally; apps no longer downloadable (third-party page on a domain unaffiliated with Rewind, so treat as secondary) | https://rewind.ai/what-happened-to-rewind/ |
| Recall: opt-in, Copilot+ PC requirements, local encrypted snapshots, Windows Hello, sensitive-info filter [V] | https://support.microsoft.com/en-us/windows/retrace-your-steps-with-recall-aa03f8a0-a78b-4b3e-b0a1-2eb8ac48701c |
| Recall extraction research (April 2026) and Microsoft's response | https://www.csoonline.com/article/4159643/microsofts-windows-recall-still-allows-silent-data-extraction.html |
| Raycast plans, AI not in Free, Memory and Screen Awareness in paid plans | https://www.raycast.com/pricing |
| Raycast usage-based AI pricing, Sep 10, 2026 | https://www.raycast.com/blog/changing-how-raycast-ai-is-priced |
| Raycast AI features, providers, local models, privacy statements [V] | https://www.raycast.com/core-features/ai |
| ChatGPT references past chats (Apr 10, 2025), Plus and Pro first, can be turned off | https://techcrunch.com/2025/04/10/openai-updates-chatgpt-to-reference-your-other-chats/ |
| ChatGPT new memory system (June 2026), Plus and Pro in the US first | https://www.thurrott.com/a-i/337052/openai-is-improving-how-chatgpts-memory-works |
| ChatGPT "Work with Apps" on macOS reads selected apps on request (Nov 14, 2024) | https://techcrunch.com/2024/11/14/chatgpt-can-now-read-some-of-your-macs-desktop-apps |
| ChatGPT plan prices (third-party list, updated 2026-06-25) | https://pricepertoken.com/subscriptions/chatgpt |
| Claude memory and chat search by plan; incognito chats | https://support.claude.com/en/articles/11817273-use-claude-s-chat-search-and-memory-to-build-on-previous-context |
| Claude plan prices; web, desktop, and mobile | https://claude.com/pricing |
| Claude Desktop connects to local MCP servers | https://modelcontextprotocol.io/quickstart/user |
