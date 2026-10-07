import { assert } from "#ui/assert.ts";
import { describe, expect, it } from "vitest";
import type { ForgeInfo } from "@gitbutler/but-sdk";
import { forgeAuthFailure, forgeDestination, forgeHasAccount, isCloudForge } from "./forge.ts";

const info = { name: "github", baseUrl: "https://github.com/acme/repo" } as ForgeInfo;
describe("forge", () => {
	it("uses the project's forge and the review's host", () => {
		expect(forgeDestination(info, "https://git.example.com/acme/repo/-/merge_requests/5")).toEqual({
			name: "github",
			label: "GitHub",
			host: "git.example.com",
		});
	});
	it("does not identify an enterprise host as a cloud forge", () => {
		const destination = assert(forgeDestination(info, "https://git.example.com/acme/repo/pull/5"));
		expect(isCloudForge(destination)).toBe(false);
	});
	it("recognizes GitHub cloud", () => {
		expect(isCloudForge(assert(forgeDestination(info)))).toBe(true);
	});
	it("recognizes Bitbucket cloud", () => {
		const destination = assert(
			forgeDestination(
				{ ...info, name: "bitbucket" },
				"https://bitbucket.org/acme/repo/pull-requests/5",
			),
		);
		expect(isCloudForge(destination)).toBe(true);
	});
	it("recognizes Gitea destinations without treating them as cloud", () => {
		const destination = assert(
			forgeDestination(
				{ ...info, name: "gitea", baseUrl: "https://gitea.hostarm.com/acme/repo" },
				"https://gitea.hostarm.com/acme/repo/pulls/5",
			),
		);
		expect(destination).toEqual({
			name: "gitea",
			label: "Gitea",
			host: "gitea.hostarm.com",
		});
		expect(isCloudForge(destination)).toBe(false);
	});
	it("recognizes gitea.com as the cloud host", () => {
		const destination = assert(
			forgeDestination({
				...info,
				name: "gitea",
				baseUrl: "https://gitea.com/acme/repo",
			}),
		);
		expect(isCloudForge(destination)).toBe(true);
	});
	it("counts a self-hosted Gitea account even though the host is not gitea.com", () => {
		const destination = assert(
			forgeDestination(
				{ ...info, name: "gitea", baseUrl: "https://third.hostarm.com/web/webarm" },
				"https://gitea.hostarm.com/web/webarm/pulls/5",
			),
		);
		expect(isCloudForge(destination)).toBe(false);
		expect(forgeHasAccount(destination, [{ provider: "gitea" }])).toBe(true);
		expect(forgeHasAccount(destination, [])).toBe(false);
	});
	it("distinguishes backend login failures from network and permission errors", () => {
		expect(
			forgeAuthFailure(
				new Error(
					"Error invoking remote method 'listReviews': Error: GitHub authentication failed.",
				),
			),
		).toBe("rejected");
		expect(
			forgeAuthFailure(
				new Error(
					"Not authenticated with GitLab. Connect your account under Settings → Integrations.",
				),
			),
		).toBe("missing");
		expect(forgeAuthFailure(new Error("Unable to connect to GitHub."))).toBeNull();
		expect(
			forgeAuthFailure(
				new Error("A GitHub organization has restricted access for the GitButler OAuth app."),
			),
		).toBeNull();
	});
	it("treats a GitLab review-listing refusal as terminal", () => {
		expect(
			forgeAuthFailure(
				new Error(
					"Error invoking remote method 'listReviews': Error: GitLab refused access for the token.",
				),
			),
		).toBe("rejected");
		expect(
			forgeAuthFailure(new Error("Failed to list open merge requests: HTTP 403 Forbidden")),
		).toBeNull();
		expect(forgeAuthFailure(new Error("Unable to connect to GitLab."))).toBeNull();
		expect(
			forgeAuthFailure(
				new Error("GitLab could not find this project, or your token cannot see it."),
			),
		).toBeNull();
	});
});
