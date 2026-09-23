# October tickets

These files are the source of truth for the October board. `scripts/team/gitlab_sync.py` turns every ticket here into a GitLab issue, adds the labels and milestone, assigns it, and keeps a lane hub issue per person that links all of that person's tickets. Edit the file, then run the sync; do not rewrite a ticket's description in GitLab by hand (status, comments, and merge request links belong in GitLab).

| File | Owner | Focus |
|---|---|---|
| `anurup-vault-search.md` | Anurup | Vault and search: one retrieval path, real text, keyword plus meaning, evaluation |
| `minh-reopen-embeddings.md` | Minh | Reopen exactly, and vectors plus chunks on every memory without a manual step |
| `kunj-command-skills-models.md` | Kunj | Screen Guide rebuilt as a command surface, skills from what worked, more and better local model use |
| `felipe-voice-onboarding-tests.md` | Felipe | One voice pipeline for every feature, production-ready onboarding and polish, product-oriented tests |
| `product-decisions.md` | Everyone | Product, research, and decision tickets |

## Weeks and milestones

| Month week | Dates | GitLab milestone |
|---|---|---|
| W1 | Sep 28 to Oct 4 | `W02-Measure` |
| W2 | Oct 5 to 11 | `W03-Build` |
| W3 | Oct 12 to 18 | `W04-Prove` (freeze Fri Oct 16) |
| W4 | Oct 19 to 25 | `W05-Retro` (Beta Wed Oct 21) |

## Ticket format

```markdown
## VS-05 Short imperative title
- assignee: anurupkumar
- labels: area::vault-search, type::feature, prio::p0
- milestone: W02-Measure
- estimate: 3h
- depends: VS-01

**Why.** The user-facing reason, in one or two sentences.

**Today.** How it works now, with file and line.

**Do.**
1. Concrete steps.

**Done when.** A check anyone can run.

**Evidence.** What to attach before moving the ticket to `status::evidence`.
```

Rules: the ID is unique and never reused; `assignee` is a GitLab username from `docs/team/roster.json`; the owner label (`owner::anurup` and so on) is added automatically from the assignee; every ticket starts in `status::ready`. Estimates are nominal hours for one person without an agent.
