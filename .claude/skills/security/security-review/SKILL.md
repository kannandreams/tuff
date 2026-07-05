---
name: security-review
version: 1.0.0
description: Review code, configuration, and infrastructure for security vulnerabilities, secrets exposure, and attack surfaces.
triggers:
  - "security review"
  - "security audit"
  - "check for vulnerabilities"
  - "secrets check"
  - "threat model"
  - "is this secure"
allowed-tools: [Read, Grep, Glob]
---

# Security Review Skill

## When to invoke this skill

Review a codebase or change for security issues — exposed secrets, injection
vectors, insecure defaults, dependency vulnerabilities, and missing access
controls. Use this before merging sensitive changes, when integrating a new
third-party dependency, or as a pre-release gate.

Do NOT use this for general code quality or style review — `code-review`
handles that. Do NOT use this for compliance audits (SOC 2, HIPAA) or
penetration testing — those require specialized tooling and scope beyond
a code-level review.

## Inputs

- code or config change to review
- authentication and authorization model
- data classification (PII, secrets, public)
- dependency manifest
- deployment environment context
- known threat model or attack surface notes

## Outputs

- vulnerability findings ordered by severity
- exposed secrets or credentials
- insecure configuration patterns
- dependency risk notes
- recommended fixes with concrete locations
- residual risk assessment

## Rules

- Prioritize issues that expose data, credentials, or allow unauthorized
  access.
- Ground findings in concrete file and line references.
- Distinguish between confirmed vulnerabilities and hardening suggestions.
- Never log or echo secrets found during review.
- Check configuration files, CI/CD pipeline configs, and documentation for
  secrets — not just source code.
- Treat third-party dependency changes as elevated risk.

## Example Workflow

1. Read the change summary and scope.
2. Scan for secrets and credentials in source, config, and CI files.
3. Review authentication and authorization paths affected by the change.
4. Check for injection vectors (SQL, command, template) in new or modified
   input handling.
5. Review dependency changes for known vulnerabilities.
6. Check error handling for information leakage.
7. Write findings in severity order with concrete fix recommendations.

## Security Review Checklist

- [ ] No secrets or credentials in source, config, or CI files
- [ ] No user input reaches SQL, shell, or templating engines unsanitized
- [ ] Authentication checks exist on all protected routes or endpoints
- [ ] Authorization boundaries are enforced, not just checked on the client
- [ ] Error messages do not leak stack traces, paths, or internal state
- [ ] New dependencies have acceptable vulnerability profiles
- [ ] Environment-specific configuration does not weaken security in prod

## Acceptance Criteria

The review is complete when it:

- documents the files, configs, and manifests inspected
- reports any exposed secret or credential found without repeating the secret value
- checks injection paths introduced or changed in scope
- checks authentication and authorization paths affected by the change
- flags dependency risks with specific version or CVE references
- provides actionable fix recommendations for each finding
- documents residual risk where a fix is deferred
