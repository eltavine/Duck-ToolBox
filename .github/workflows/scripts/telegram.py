#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path


def _escape_html(text: str) -> str:
    return text.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def _truncate(text: str, limit: int) -> str:
    if limit <= 0:
        return ""
    if len(text) <= limit:
        return text
    if limit <= 3:
        return text[:limit]
    return text[: limit - 3].rstrip() + "..."


def _first_paragraph(lines: list[str]) -> str:
    paragraphs: list[str] = []
    current: list[str] = []
    for raw in lines:
        line = raw.strip()
        if not line:
            if current:
                paragraphs.append(" ".join(current))
                current = []
            continue
        current.append(line)
    if current:
        paragraphs.append(" ".join(current))
    return paragraphs[0] if paragraphs else ""


def _render_caption(
    subject_text: str,
    summary_text: str,
    commit_author: str,
    git_branch: str,
    short_sha: str,
    action_url: str,
) -> str:
    commit_lines = [subject_text] if subject_text else []
    if summary_text:
        commit_lines.extend(["", summary_text])
    commit_block = _escape_html("\n".join(commit_lines))

    lines = [
        "Commit:",
        f"<pre>{commit_block}</pre>",
        f"Author: <b>{_escape_html(commit_author)}</b>",
        f"Branch: <b>{_escape_html(git_branch)}@{_escape_html(short_sha)}</b>",
        f'GitHub Action: <a href="{_escape_html(action_url)}">Open run</a>',
    ]
    return "\n".join(lines)


def build_caption(artifact_path: str) -> None:
    commit_message = os.environ.get("COMMIT_MESSAGE", "").strip()
    commit_author = os.environ.get("COMMIT_AUTHOR", "").strip() or "Unknown author"
    git_branch = os.environ.get("GIT_BRANCH", "").strip() or "unknown"
    git_sha = os.environ.get("GIT_SHA", "").strip()
    action_url = os.environ.get("GITHUB_ACTION_URL", "").strip()

    if not artifact_path or not Path(artifact_path).is_file():
        print("Module artifact was not downloaded successfully.", file=sys.stderr)
        sys.exit(1)

    if commit_message:
        lines = commit_message.splitlines()
        subject = lines[0].strip()
        body_lines = lines[1:]
    else:
        subject = f"Build artifact: {Path(artifact_path).name}"
        body_lines = []

    summary = _first_paragraph(body_lines)
    short_sha = git_sha[:7] if git_sha else "unknown"

    max_caption_length = 1024
    caption = _render_caption(
        subject, summary, commit_author, git_branch, short_sha, action_url
    )

    if len(caption) > max_caption_length and summary:
        without_summary = _render_caption(
            subject, "", commit_author, git_branch, short_sha, action_url
        )
        summary_budget = max_caption_length - len(without_summary) - 2
        summary = _truncate(summary, summary_budget)
        caption = _render_caption(
            subject, summary, commit_author, git_branch, short_sha, action_url
        )

    if len(caption) > max_caption_length:
        without_subject = _render_caption(
            "", summary, commit_author, git_branch, short_sha, action_url
        )
        subject_budget = max_caption_length - len(without_subject)
        subject = _truncate(subject, subject_budget)
        caption = _render_caption(
            subject, summary, commit_author, git_branch, short_sha, action_url
        )

    print(caption, end="")


def validate_response(response_file: str) -> None:
    try:
        with open(response_file, "r", encoding="utf-8") as fh:
            payload = json.load(fh)
    except (OSError, json.JSONDecodeError) as exc:
        print(f"Failed to read response file: {exc}", file=sys.stderr)
        sys.exit(1)

    if not payload.get("ok"):
        print(
            payload.get("description", "Telegram API returned ok=false"),
            file=sys.stderr,
        )
        sys.exit(1)

    result = payload.get("result") or {}
    message_id = result.get("message_id")
    if message_id is None:
        print(
            "Telegram API succeeded but did not return a message_id.", file=sys.stderr
        )
        sys.exit(1)

    print(f"Telegram message_id: {message_id}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    caption_parser = subparsers.add_parser(
        "build-caption", help="Build Telegram caption"
    )
    caption_parser.add_argument(
        "--artifact-path", required=True, help="Path to the module zip artifact"
    )

    validate_parser = subparsers.add_parser(
        "validate-response", help="Validate Telegram API response"
    )
    validate_parser.add_argument(
        "--response-file", required=True, help="Path to the JSON response file"
    )

    args = parser.parse_args()

    if args.command == "build-caption":
        build_caption(args.artifact_path)
    elif args.command == "validate-response":
        validate_response(args.response_file)


if __name__ == "__main__":
    main()
