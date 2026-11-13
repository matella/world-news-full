# Private article intake services

`IntakeService` is an authenticated internal API for #20. The web process does not
start extraction or polling; UI integration is #22/#29 and jobs are #21. No Jarvis,
Node or AI provider participates. These services do not republish publisher text.

- `register_link(session, ManualLink)` records metadata, source URL and the editor's
  own notes without a feed or article HTTP fetch. URL-v1 identity matches #19 and
  rejects an existing identity with Conflict rather than overwriting it. Disabled
  sources can retain manual notes; they cannot be fetched. No original-origin claim.
- `head` returns intake version, last snapshot pointer and extraction state.
- `extract(session, article, expected_version)` either records a metadata-only denial,
  extracts private text, or records a safe failure category. It requires current
  documented licensed/original storage, a configured source hostname match and an
  exact same-origin redirect chain. Source/session/version changes during HTTP reject
  completion with no stale text write. The old snapshot remains; retry is explicit.
- `manual(session, article, expected_version, own_notes, optional OriginalDraft)`
  appends authored notes and optionally creates a separate original draft and initial
  revision link in the same transaction. It works after denied/failed extraction.
  Failed linking leaves no partial draft/revision/note. A manual note alone does not
  change extraction outcome. User interfaces must label this text editor-authored;
  it is not an upload API for unlicensed publisher bodies.
- `history` returns bounded metadata pages (id cursor, PageSize 1–100), no bodies.
  `snapshot` returns private notes and retained extracted text only if current storage
  rights allow it. `text_withheld` indicates policy denial or removed text. Display
  policy is captured separately; private storage permission never authorizes public
  rendering. No public route/view exposes these tables.

Extraction limits: 2 MiB downloaded HTML, shared 15-second HTTP budget and 3 redirects,
3-second parse budget/two parser slots retained until blocking work ends; service
network/parser phase capped at 20 seconds. At most 100,000 DOM nodes/depth 128, one
article/main, 500 paragraphs, 20,000 characters per paragraph, 200,000 total UTF-8 text
bytes, 500 title characters and 20 author names of 500 characters. UTF-8 HTML only;
unsupported charset/content type or ambiguous/no paragraphs fail without truncation.
Only explicit visible p text is retained; lists, tables, script-only pages and
site-specific layouts may need manual fallback. HTML strings/attributes are not output.
Publication metadata is parsed input, not verified historical evidence.

Private provenance snapshots contain source version/policy, actor/time, response
hash, text hash, title/authors/date/final article URL and paragraph count. Original
source_articles URLs/title/language/body remain unchanged; extracted final URL is
only in extraction evidence. Notes have their own manual kind. Retained text is in
intake_text, separate from immutable snapshots/draft links/removal records. It is
never selected by an anonymous reader. Do not log notes, raw URLs, HTML, text or
transport errors. Treat all source text as untrusted data in future AI adapters.

Updating source storage to unknown/metadata-only/invalid evidence automatically
removes retained intake text for that source via a fixed-search-path, non-PUBLIC
security-definer trigger. Its removal ledger records source version and reason;
registry_actions supplies the configured editor/actor for normal registry changes.
No identity is invented for direct operator SQL. The affected intake extraction state becomes metadata_only. Notes/original drafts
remain authored work. Re-authorizing storage does not restore text; a fresh extraction is needed.
Backing up retained text requires compatible rights, retention and removal procedures
(#47). The trigger does not alter legacy text outside these new intake tables.

Role needs: authenticated editor pool retains existing identity/session/editorial
grants. Add SELECT sources/source_origin_links/source_articles/intake_snapshots/
intake_text/intake_text_removals/intake_draft_links; INSERT source_articles/
intake_snapshots/intake_text/intake_draft_links; UPDATE only intake_version,
current_intake_id,extraction_state on source_articles; USAGE intake_snapshots_id_seq;
EXECUTE lock_source_for_fetch(text). No configuration UPDATE or retained text UPDATE/
DELETE is needed. Anonymous reader gains nothing. Migration owner alone installs the
policy-cleanup trigger; live grants and role deployment are not performed here.

Migration 9 is additive but changing source policy has intentional text-removal
semantics after deployment. Review those effects and backups before a live rollout.
Rollback is a matching-schema forward fix/verified restore, not dropping audit rows
or silently bypassing exact schema readiness. See ADR 0008 and the evidence record.
