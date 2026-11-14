# Code integration and deployment during Rust migration

The owner authorized merging the implementation PRs and closing completed issues.
Code integration now proceeds separately from changing a running environment.

The two old push-to-main deployment workflows are retired. They could stop the
Compose stack, rebuild/start it, prune host images and run a separate Kubernetes
image update. Their target environment, archive state and rollback readiness are
not established by repository source inspection. A code merge must not trigger
those host effects while the replacement publishing/recovery path is unfinished.

This change removes those workflow entry points only. It does not contact a runner,
stop/restart any container or writer, alter a database/PVC/secret/grant, switch reader
traffic or prune images. Existing running processes continue independently of this
repository edit. Legacy scripts/manifests remain source history, not an endorsed
production release procedure.

A manual release-guard workflow explains the hold and intentionally fails without
performing a remote action. The source-policy workflow verifies the retired files
stay absent, the approved manual hold stays exact and no known legacy host/deploy
commands occur in workflows. Four local tests cover allowed CI, trigger resurrection,
manual-hold changes and known deployment commands. This narrow source check is not
a generic YAML validator, branch-protection rule or complete security audit.

Merge validated implementation PRs in dependency order, using expected head SHAs.
Keep their review/evidence linked. Close an issue only once all of its acceptance
criteria are met; partial work, runtime checks still missing and real release
acceptance stay open. Do not mark a component deployed merely because it is merged.

The supported Rust image/deployment path remains roadmap #46. Restore/upgrade/
rollback rehearsal and complete publishing/migration acceptance remain #47–#49.
Before enabling a real release workflow, prepare exact images/versions, affected
environment and identities, archive reconciliation, backup/restore evidence, writer
ownership, explicit scheduling and rollback/traffic changes. Obtain the required
concrete environment authorization before executing live effects. Do not silently
re-enable the old push deployment or use scripts/deploy.sh as a shortcut.
