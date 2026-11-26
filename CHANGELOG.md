# Changelog

<!--
==============================================================================
CHANGELOG.md - Release history
==============================================================================
Description: Notable changes in each release, newest first
Author: Matt Barham
Created: 2026-09-27
Modified: 2026-11-26
Version: 1.6.0
==============================================================================
Document Type: Changelog
Audience: Operator, Module Developer
Status: Active (living document)
==============================================================================
-->

All notable changes to this project are documented here. The format follows
[Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/), and versions
follow [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html). What
counts as a breaking change for a Spoke module is defined in the Spoke hub's
ADR-029.

## [1.6.0] - 2026-11-26

### Fixed

- Merge genotyped calls correctly and keep missing calls missing (#61)

## [1.5.8] - 2026-11-26

### Fixed

- Create the whole schema on a fresh install (#55)
- Carry uploaded polygenic scores into results (#57)
- Wipe inputs after jobs and harden job endpoints (#58)

## [1.5.7] - 2026-11-25

### Changed

- Maintenance only; no user-visible changes.

## [1.5.6] - 2026-11-16

### Changed

- Maintenance only; no user-visible changes.

## [1.5.5] - 2026-10-26

### Fixed

- Restart genetics-frontend unless stopped (#49)

## [1.5.4] - 2026-10-24

### Fixed

- Build DATABASE_URL from the app-role secret file (#47)

## [1.5.3] - 2026-10-20

### Changed

- Maintenance only; no user-visible changes.

## [1.5.2] - 2026-10-02

### Changed

- Maintenance only; no user-visible changes.

## [1.5.1] - 2026-09-27

### Fixed

- Report the release version instead of a hardcoded v1.0.0 (#40)

## [1.5.0] - 2026-09-27

### Baseline

- First versioned release (hub ADR-029). Earlier history is in the git log.
