#!/usr/bin/env python3
"""
Generate a release risk assessment report for a Goose release PR.

Usage:
    .release_risk_report.py --version 1.27.0
    .release_risk_report.py --version 1.27.0 --output report.md
    .release_risk_report.py --version 1.27.0 --pr 7611
"""

import argparse
import json
import re
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor, as_completed

REPO = "aaif-goose/goose"

# Paths considered documentation-only
DOC_PATTERNS = [
    r"^documentation/",
]

# Paths considered high-risk core code
CORE_PATHS = [
    "crates/goose/src/agents/",
    "crates/goose/src/providers/",
    "crates/goose-server/",
    "crates/goose-cli/",
    "crates/goose/src/session",
    "crates/goose/src/permission",
]

# Dependency lock files (safe to skip — lock files only, not manifests)
DEP_LOCK_FILES = [
    "Cargo.lock",
    "package-lock.json",
    "yarn.lock",
    "pnpm-lock.yaml",
]


def run_gh(args: list[str]) -> str:
    """Run a gh CLI command and return stdout."""
    result = subprocess.run(
        ["gh"] + args,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        print(f"  [warn] gh command failed: gh {' '.join(args)}", file=sys.stderr)
        print(f"         {result.stderr.strip()}", file=sys.stderr)
        return ""
    return result.stdout.strip()


def find_release_pr(repo: str, version: str) -> int | None:
    """Find the open release PR matching the given version string."""
    output = run_gh([
        "pr", "list", "--repo", repo, "--state", "open",
        "--json", "number,title", "--limit", "100",
    ])
    if not output:
        return None
    prs = json.loads(output)
    for pr in prs:
        if version.lower() in pr["title"].lower():
            return pr["number"]
    return None


def get_pr_body(repo: str, pr_number: int) -> str:
    """Get the body of a PR."""
    return run_gh(["pr", "view", str(pr_number), "--repo", repo, "--json", "body", "--jq", ".body"])


def extract_pr_numbers(body: str) -> list[int]:
    """Extract PR numbers from the 'Changes in This Release' section."""
    section_match = re.search(r"## Changes in This Release\s*\n", body)
    if not section_match:
        print("[error] Could not find 'Changes in This Release' section", file=sys.stderr)
        return []

    section_text = body[section_match.end():]
    end_match = re.search(r"\n---|\n##", section_text)
    if end_match:
        section_text = section_text[:end_match.start()]

    pr_numbers = re.findall(r"\(#(\d+)\)", section_text)
    return [int(n) for n in pr_numbers]


def is_doc_only(files: list[dict]) -> bool:
    """Return True if all changed files are documentation/non-code."""
    if not files:
        return False
    for f in files:
        path = f.get("path", "")
        if not any(re.search(pat, path) for pat in DOC_PATTERNS):
            return False
    return True


def is_deps_only(files: list[dict]) -> bool:
    """Return True if all changed files are dependency/lock files."""
    if not files:
        return False
    for f in files:
        path = f.get("path", "")
        basename = path.rsplit("/", 1)[-1]
        if basename not in DEP_LOCK_FILES:
            return False
    return True


def assess_risk(files: list[dict]) -> dict:
    """Assess risk level and return risk info dict."""
    total_additions = sum(f.get("additions", 0) for f in files)
    total_deletions = sum(f.get("deletions", 0) for f in files)
    total_lines = total_additions + total_deletions
    num_files = len(files)

    paths = [f.get("path", "") for f in files]

    # Classify files
    core_files = [p for p in paths if any(p.startswith(cp) for cp in CORE_PATHS)]
    test_files = [p for p in paths if "test" in p.lower() or "snap" in p.lower()]
    dep_files = [p for p in paths if p.rsplit("/", 1)[-1] in DEP_LOCK_FILES]
    prod_files = [p for p in paths if p not in test_files and p not in dep_files]

    # Collect risk factors
    factors = []

    if total_lines > 500:
        factors.append(f"Large change ({total_lines} lines)")
    elif total_lines > 200:
        factors.append(f"Medium-sized change ({total_lines} lines)")

    if num_files > 10:
        factors.append(f"Touches {num_files} files")

    if core_files:
        factors.append(f"Modifies core code: {', '.join(summarize_paths(core_files))}")

    if dep_files:
        factors.append(f"Dependency changes: {', '.join(f.rsplit('/', 1)[-1] for f in dep_files)}")

    if len(prod_files) > 0 and len(test_files) == 0:
        factors.append("No test files changed")

    # Determine risk level
    risk_score = 0
    if total_lines > 500:
        risk_score += 2
    elif total_lines > 200:
        risk_score += 1
    if num_files > 10:
        risk_score += 1
    if core_files:
        risk_score += 2
    if len(prod_files) > 0 and len(test_files) == 0:
        risk_score += 1

    if risk_score >= 4:
        level = "HIGH"
    elif risk_score >= 2:
        level = "MEDIUM"
    else:
        level = "LOW"

    # Summarize key paths (deduplicate to directory level)
    key_paths = summarize_paths(paths)

    return {
        "level": level,
        "score": risk_score,
        "factors": factors,
        "total_additions": total_additions,
        "total_deletions": total_deletions,
        "total_lines": total_lines,
        "num_files": num_files,
        "key_paths": key_paths,
    }


def summarize_paths(paths: list[str], max_display: int = 5) -> list[str]:
    """Summarize a list of file paths to their common directories."""
    dirs = set()
    for p in paths:
        parts = p.split("/")
        if len(parts) > 3:
            dirs.add("/".join(parts[:3]) + "/")
        else:
            dirs.add(p)
    result = sorted(dirs)
    if len(result) > max_display:
        return result[:max_display] + [f"... and {len(result) - max_display} more"]
    return result


def extract_testing_section(body: str) -> str:
    """Extract the testing section from a PR description."""
    if not body:
        return "No"

    # Try various common testing section headers
    patterns = [
        r"##\s*Test(?:ing)?\s*(?:Plan|Strategy)?[ \t]*\r?\n",
        r"##\s*How (?:Has This Been |to )Test(?:ed)?[ \t]*\r?\n",
        r"##\s*Verification[ \t]*\r?\n",
    ]

    for pattern in patterns:
        match = re.search(pattern, body, re.IGNORECASE)
        if match:
            section = body[match.end():]
            # Stop at next heading or HR
            end = re.search(r"\n##\s|\n---", section)
            if end:
                section = section[:end.start()]
            section = section.strip()
            if section:
                # Clean up markdown checkboxes and compress whitespace
                section = re.sub(r"<!--.*?-->", "", section, flags=re.DOTALL).strip()
                # Remove empty checkbox lines like "- [ ] Feature"
                lines = [l for l in section.split("\n") if l.strip()]
                cleaned = "\n".join(lines).strip()
                if cleaned and not re.match(r"^[\s\-\[\]xX]*$", cleaned):
                    return cleaned
    return "No"


def get_pr_details(repo: str, pr_number: int) -> dict | None:
    """Fetch title, body, author, files, and approvers for a single PR."""
    output = run_gh([
        "pr", "view", str(pr_number), "--repo", repo,
        "--json", "number,title,body,author,files",
    ])
    if not output:
        return None

    data = json.loads(output)

    # Get reviewers who approved
    reviews_output = run_gh([
        "api", f"repos/{repo}/pulls/{pr_number}/reviews",
        "--jq", '[.[] | select(.state == "APPROVED") | .user.login] | unique | join(", ")',
    ])

    files = data.get("files", []) or []
    body = (data.get("body") or "").strip()

    return {
        "number": data["number"],
        "title": data["title"],
        "description": body,
        "author": data.get("author", {}).get("login", "unknown"),
        "approvers": reviews_output if reviews_output else "none",
        "files": files,
        "testing": extract_testing_section(body),
    }


def generate_report(repo: str, release_pr: int, pr_details: list[dict], skipped_docs: list[dict]) -> str:
    """Generate a risk assessment markdown report."""
    lines = [
        f"# Release Risk Assessment — PR #{release_pr}",
        "",
        f"**Repository:** {repo}",
        f"**Total PRs in release:** {len(pr_details) + len(skipped_docs)}",
        f"**Assessed PRs:** {len(pr_details)} (skipped {len(skipped_docs)} doc-only)",
        "",
    ]

    # Summary counts by risk
    risk_counts = {"HIGH": 0, "MEDIUM": 0, "LOW": 0}
    for pr in pr_details:
        risk_counts[pr["risk"]["level"]] += 1

    lines.append(f"**Risk summary:** {risk_counts['HIGH']} High, {risk_counts['MEDIUM']} Medium, {risk_counts['LOW']} Low")
    lines.append("")
    lines.append("---")
    lines.append("")

    # Sort by risk score descending, then by PR number
    pr_details_sorted = sorted(pr_details, key=lambda x: (-x["risk"]["score"], x["number"]))

    for i, pr in enumerate(pr_details_sorted, 1):
        risk = pr["risk"]
        risk_badge = {"HIGH": "HIGH RISK", "MEDIUM": "MEDIUM RISK", "LOW": "LOW RISK"}[risk["level"]]

        lines.append(f"## {i}. #{pr['number']} — {pr['title']}  `[{risk_badge}]`")
        lines.append("")
        lines.append(f"- **Author:** @{pr['author']}")
        lines.append(f"- **Approved by:** {pr['approvers']}")

        # Include PR description for MEDIUM and HIGH risk PRs
        if risk["level"] in ("HIGH", "MEDIUM") and pr.get("description"):
            desc = pr["description"].replace("\n", " ").strip()
            if len(desc) > 500:
                desc = desc[:500] + "..."
            lines.append(f"- **Description:** {desc}")

        lines.append(f"- **Files changed:** {risk['num_files']} (+{risk['total_additions']} / -{risk['total_deletions']})")

        # Include individual file paths for MEDIUM and HIGH risk PRs
        if risk["level"] in ("HIGH", "MEDIUM") and pr.get("files"):
            file_lines = [f"  - `{f['path']}` (+{f.get('additions', 0)}/-{f.get('deletions', 0)})" for f in pr["files"]]
            lines.append(f"- **Files:**")
            lines.extend(file_lines)
        else:
            lines.append(f"- **Key paths:** {', '.join(risk['key_paths']) if risk['key_paths'] else 'N/A'}")

        if risk["factors"]:
            lines.append(f"- **Risk factors:** {'; '.join(risk['factors'])}")
        else:
            lines.append("- **Risk factors:** None notable")

        # Testing section
        testing = pr["testing"]
        if testing == "No":
            lines.append("- **Testing:** No")
        else:
            # Show first 300 chars of testing section
            testing_summary = testing.replace("\n", " ").strip()
            if len(testing_summary) > 300:
                testing_summary = testing_summary[:300] + "..."
            lines.append(f"- **Testing:** {testing_summary}")

        lines.append(f"- **Link:** https://github.com/{repo}/pull/{pr['number']}")
        lines.append("")

    # Skipped docs section
    if skipped_docs:
        lines.append("---")
        lines.append("")
        lines.append(f"## Skipped: {len(skipped_docs)} documentation-only PRs")
        lines.append("")
        for pr in skipped_docs:
            lines.append(f"- #{pr['number']} — {pr['title']} (@{pr['author']})")
        lines.append("")

    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description="Generate a release risk assessment report")
    parser.add_argument("--pr", type=int, help="Release PR number (if omitted, searches for open release PRs)")
    parser.add_argument("--version", required=True, help="Version string to search for (e.g. 1.27.0)")
    parser.add_argument("--repo", default=REPO, help=f"GitHub repo (default: {REPO})")
    parser.add_argument("--output", "-o", help="Output file (default: stdout)")
    parser.add_argument("--workers", type=int, default=5, help="Number of parallel workers (default: 5)")
    args = parser.parse_args()

    # Step 1: Find the release PR
    release_pr = args.pr
    if not release_pr:
        print(f"Searching for open PR with '{args.version}' in title...", file=sys.stderr)
        release_pr = find_release_pr(args.repo, args.version)
        if not release_pr:
            print(f"[error] No open PR found with '{args.version}' in title", file=sys.stderr)
            sys.exit(1)
    print(f"Using release PR #{release_pr}", file=sys.stderr)

    # Step 2: Get PR body and extract linked PR numbers
    body = get_pr_body(args.repo, release_pr)
    if not body:
        print("[error] Could not fetch PR body", file=sys.stderr)
        sys.exit(1)

    pr_numbers = extract_pr_numbers(body)
    if not pr_numbers:
        print("[error] No PR numbers found in the release body", file=sys.stderr)
        sys.exit(1)

    print(f"Found {len(pr_numbers)} PRs in the release notes", file=sys.stderr)

    # Step 3: Fetch details for each PR in parallel
    all_prs = []
    failed = []

    with ThreadPoolExecutor(max_workers=args.workers) as executor:
        futures = {
            executor.submit(get_pr_details, args.repo, num): num
            for num in pr_numbers
        }
        for future in as_completed(futures):
            num = futures[future]
            try:
                detail = future.result()
                if detail:
                    all_prs.append(detail)
                    print(f"  Fetched #{num}: {detail['title'][:60]}", file=sys.stderr)
                else:
                    failed.append(num)
            except Exception as e:
                print(f"  [error] Failed to fetch #{num}: {e}", file=sys.stderr)
                failed.append(num)

    if failed:
        print(f"\n[warn] Failed to fetch {len(failed)} PRs: {failed}", file=sys.stderr)

    # Step 4: Filter and assess
    assessed_prs = []
    skipped_docs = []

    for pr in all_prs:
        if is_doc_only(pr["files"]):
            skipped_docs.append(pr)
            print(f"  [skip] #{pr['number']}: doc-only", file=sys.stderr)
        elif is_deps_only(pr["files"]):
            skipped_docs.append(pr)
            print(f"  [skip] #{pr['number']}: deps-only", file=sys.stderr)
        else:
            pr["risk"] = assess_risk(pr["files"])
            assessed_prs.append(pr)

    skipped_docs.sort(key=lambda x: x["number"])

    print(f"\nAssessed {len(assessed_prs)} PRs, skipped {len(skipped_docs)} (doc/deps-only)", file=sys.stderr)

    # Step 5: Generate report
    report = generate_report(args.repo, release_pr, assessed_prs, skipped_docs)

    if args.output:
        with open(args.output, "w") as f:
            f.write(report)
        print(f"Report written to {args.output}", file=sys.stderr)
    else:
        print(report)


if __name__ == "__main__":
    main()
