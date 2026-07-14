# Security Policy

## Reporting a Vulnerability

If you discover a security vulnerability in Observable, please **do not** open a public GitHub
issue. Instead, report it privately using
[GitHub Security Advisories](../../security/advisories/new) for this repository, or contact the
maintainers directly.

Please include:
- A description of the vulnerability and its potential impact.
- Steps to reproduce, including affected service(s) or component(s).
- Any relevant logs, request/response samples, or proof-of-concept code.

We will acknowledge your report as soon as possible and keep you updated as we investigate and
address the issue. We ask that you give us a reasonable amount of time to address a report
before any public disclosure.

## Supported Versions

This project is at an early (0.x) release stage. Security fixes are made against the `main`
branch; there is no long-term-support branch yet.

## Deployment Security Notes

This is a self-hosted platform — operators are responsible for their own deployment's security
posture. Before deploying to production, in particular:

- **Set all required secrets explicitly.** Do not rely on any default values shipped in Helm
  chart `values.yaml` files or example env files for session-signing keys, database credentials,
  or API tokens — override every secret-bearing value for your environment.
- **Review `charts/*/values.yaml`** for any value that should be tenant- or
  environment-specific before installing.
- **Keep dependencies current.** Run `bash scripts/local-ci.sh` (which includes dependency and
  container image checks) before deploying a new build.
