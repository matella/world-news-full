# ADR 0019: evaluate length without rewriting articles

Status: accepted for implementation. Related: #28; extends ADRs 0016 and 0018.

Article length is editorial guidance, not a content-generation target or quality
score. Count authored body text only, keep original strings unchanged and require
a public reason for configured exceptions. Never pad an article, slice paragraphs
or hide truncation behind an apparently edited full article.

Use a versioned Unicode count policy. NFC normalization applies only to counting;
stored original-language prose remains byte-for-byte intact. Treat internal
apostrophes/hyphens and formatted numbers explicitly. Validate the rule for French,
English, Dutch and German; other language tags remain approximate and do not get
a misleading numeric reading-time estimate. See docs/article-length.md.

Budgets live in the existing editorial policy: brief 80–150, standard 350–600 and
analysis 800–1,200 by default. Optional language format maps resolve exact tag,
primary language, then default formats. Do not equate a lead edition slot with
opinion or analysis authorship. Reading estimates use a configurable rate, initially
200 words/minute, rounded up. Edition reading budgets remain separate (#30).

Every new authored revision records its count/range/policy fingerprint and optional
public override reason in immutable provenance. A length-only change clones prose,
sourcing and exact citation versions into a new candidate and resets approval.
Source/classification changes retain an existing assessment; a prose edit does not
inherit an override. Old imports stay readable without invented past assessments.
Publication rechecks the actual revision and active policy. Stale assessments need
new review; zero-word prose cannot be rescued by a reason. This is still a human
editorial decision rather than automatic certification of content quality.

Private pages show count, range, warnings and a reasoned-override form protected by
the existing live role/session/version/CSRF/Origin checks. Public pages show reader
information and retained public format choices, not operator forms or instructions.
Migration 17 appends an explicitly allowlisted assessment projection to the public
view without exposing revision actors or unrelated private provenance.

Use the configured policy when constructing editorial services. The explicit
reference constructor uses bundled defaults; manual intake remains a nonpublishing
draft and never silently supplies an exception. Compatible gated writer images are
required for rollback; never restore a writer that bypasses sourcing/length rules.
Live migrations, grants, writer ownership and deployment remain #47–#49 actions.
