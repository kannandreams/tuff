---
title: Privacy
description: What Tuff, its Claude Code plugin, and this website do with your data.
---

Last updated 3 September 2026.

## The short version

Tuff collects nothing. There is no account, no telemetry, no usage reporting, and no analytics in the command-line tool or in the Claude Code plugin. Nothing about your projects, your capabilities, or your machine is sent to the author.

## The Claude Code plugin

The plugin is four text files: a manifest, a readme, a marketplace listing, and one skill. It contains no hooks, no MCP servers, and no executables, so it runs no code of its own and makes no network requests. Installing it copies those files onto your machine and nothing else.

The plugin does not include the Tuff binary. It expects the `tuff` command to already be on your PATH, and the skill tells the agent to ask you before installing it.

## The command-line tool

Tuff makes a network request only when a command you ran needs one, and only to the host that command names:

- Installing a capability from a Git repository contacts that repository.
- `tuff mcp search`, and `tuff add mcp` with a name that is not a built-in catalog entry, contact the official MCP registry. `--registry` points them at a different one.
- `tuff mcp doctor` contacts each MCP server you have configured, in order to verify it actually answers.
- `tuff pack push` and `tuff pack pull` contact the OCI registry named in the reference.

No request carries anything beyond what the operation needs, and none of them reach the author or any service the author runs.

## Secrets

A Tuff manifest has no field a secret value can occupy. Environment variables and authentication headers are recorded as references to variable names, never as values, and a literal secret is refused when the manifest is parsed rather than stored and warned about later. Header values are read from your environment at the moment a request is made, so they exist in the process and not in any file Tuff writes.

## What Tuff stores on your machine

Tuff writes `tuff.lock`, your capability files under `.agents/` and the harness directories, and a project configuration file. Those are ordinary files in your repository, and you decide whether to commit them.

It also keeps a machine-local cache of capability content in your platform's standard cache and state directories, which is how drift detection compares an installed file against its baseline. That cache is disposable; `tuff cache` manages it, and deleting it costs nothing but a re-fetch.

## This website

This site loads no analytics and sets no tracking cookies.

It is hosted on Cloudflare Pages, which processes ordinary request information such as your IP address and browser user agent in the course of serving pages, as any web host does.

Fonts are loaded from Google Fonts, which means your browser requests them from a Google server and that server sees the request. This is the only third-party resource the site loads.

## Changes and questions

Material changes to this page will be noted in its last-updated date. Questions about anything here belong in a [GitHub issue](https://github.com/kannandreams/tuff/issues), where the answer is public and useful to the next person who asks.
