import { describe, expect, it } from "vitest";
import { ciChecksSummaryUrl } from "#ui/ci.ts";
import type { ForgeInfo } from "@gitbutler/but-sdk";

const forge = (name: ForgeInfo["name"]): ForgeInfo => ({
	name,
	baseUrl: `https://example.com/acme/repo`,
	commitUrlPath: "/commit/",
	prUrlPath: name === "gitea" ? "/pulls/" : "/pull/",
	unit: { name: "Pull request", abbr: "PR", symbol: "#" },
	posthogLabel: "PR",
	capabilities: {
		checks: true,
		repoInfo: true,
		prService: true,
		listService: true,
		reviewComments: false,
		reviewManagement: false,
	},
});

describe("ciChecksSummaryUrl", () => {
	it("links GitHub PRs to the checks tab", () => {
		expect(ciChecksSummaryUrl(38, forge("github"))).toBe(
			"https://example.com/acme/repo/pull/38/checks",
		);
	});

	it("links Gitea PRs to the pull request page where statuses appear", () => {
		expect(ciChecksSummaryUrl(38, forge("gitea"))).toBe(
			"https://example.com/acme/repo/pulls/38",
		);
	});

	it("returns null for forges without a known checks summary URL", () => {
		expect(ciChecksSummaryUrl(38, forge("gitlab"))).toBeNull();
	});
});
