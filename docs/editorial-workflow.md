# Manual article review, publication and withdrawal

Related: #29, ADR 0027. Start at `/admin/editor`; `/admin/publishing` redirects to
the private review queue. Open an article’s **Relecture et publication** page at
`/admin/editor/story/{id}/workflow`. Existing body/source/origin/thread screens are
linked from the protected workbench; this workflow never fetches sources or calls
an AI service.

1. Prepare the private body, explicit sourcing, classification and format/length.
   Previewing is nonpersistent. Saving body, sourcing, classification or length
   creates a candidate and invalidates its previous approval. Human-method proof
   and citation bindings must be explicitly reconfirmed after prose changes.
2. Inspect the workflow’s source/length blockers, classification and uncertainty
   warnings, original language and escaped reader preview. Missing/unmapped topic
   or geography is an explicit review warning; language does not infer geography.
   Configured length policy and citation publication rules remain hard gates.
3. An editor submits the candidate. A publisher approves that exact revision or
   requests changes with a private note. Approval does not publish content.
4. A publisher explicitly publishes the approved candidate with a public reason.
   The service rechecks live identity, version, evidence and length policy. Reasons
   are intentionally public; do not put private evidence in that field.
5. Correct an article through a new private candidate, sources, review and explicit
   publication with a public correction reason. Readers retain the old publication
   until that final command. Existing edition snapshots retain their chosen revision.
6. Withdraw the published article with an explicit public reason. Even if a newer
   private candidate exists, withdrawal concerns the published revision. Readers
   receive HTTP 410 and public notices, without the article body. Withdrawal does
   not delete history or offer an automatic revive action.

The workflow shows private revision/review/publication histories, each bounded to
20 entries with older-page links. Historical previews cannot publish old content.
Private notes and account identifiers appear only in authenticated editorial views;
anonymous article pages receive the allowlisted public notice projection.

A stale/invalid transition returns 409/400 with submitted action fields retained
only after renewed permission checks. Compare with current work, then reopen the
form and make a fresh decision. Old expected versions remain old; there is no
silent merge or durable browser/server autosave. Keep an external draft if needed.
Revoked sessions or reduced roles cannot use recovery to retain privileged access.

No automated publication, AI generation, edition composition, email or deployment
is performed. Future external AI material enters a private draft with the same
review and publication requirements. Source integration does not apply migration
18 to a live database; see ADR 0027 for the schema-17 rollback incompatibility.
