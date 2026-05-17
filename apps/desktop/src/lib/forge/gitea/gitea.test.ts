import { Gitea } from "$lib/forge/gitea/gitea";
import { describe, expect, test, vi } from "vitest";
import type { BackendApi } from "$lib/state/backendApi";
import type { AppDispatch } from "$lib/state/clientState.svelte";

describe("Gitea", () => {
	const backendApi = {
		endpoints: {},
		reducerPath: "backend",
		injectEndpoints: vi.fn(() => ({
			endpoints: {},
		})),
		util: { invalidateTags: vi.fn() },
	} as unknown as BackendApi;
	const dispatch = (() => {}) as AppDispatch;

	test("uses HTTPS URL for HTTPS remote", () => {
		const gitea = new Gitea({
			repo: {
				domain: "gitea.example.com",
				name: "test-repo",
				owner: "test-owner",
				protocol: "https:",
			},
			baseBranch: "main",
			backendApi,
			dispatch,
			authenticated: true,
			isLoading: false,
		});

		expect(gitea.commitUrl("abc123")).toBe(
			"https://gitea.example.com/test-owner/test-repo/commit/abc123",
		);
		expect(gitea.branch("feature").url).toBe(
			"https://gitea.example.com/test-owner/test-repo/compare/main...feature",
		);
	});

	test("preserves HTTP URL for HTTP remote", () => {
		const gitea = new Gitea({
			repo: {
				domain: "gitea.local",
				name: "test-repo",
				owner: "test-owner",
				protocol: "http:",
			},
			baseBranch: "main",
			backendApi,
			dispatch,
			authenticated: true,
			isLoading: false,
		});

		expect(gitea.commitUrl("abc123")).toBe("http://gitea.local/test-owner/test-repo/commit/abc123");
	});

	test("uses HTTPS URL for SSH remote", () => {
		const gitea = new Gitea({
			repo: {
				domain: "code.example.com",
				name: "test-repo",
				owner: "test-owner",
				protocol: "ssh:",
			},
			baseBranch: "main",
			backendApi,
			dispatch,
			authenticated: true,
			isLoading: false,
		});

		expect(gitea.commitUrl("abc123")).toBe(
			"https://code.example.com/test-owner/test-repo/commit/abc123",
		);
	});

	test("uses view URL for browser links when API host differs", () => {
		const gitea = new Gitea({
			repo: {
				domain: "git.example.com",
				name: "versanis",
				owner: "web",
				protocol: "ssh:",
			},
			baseBranch: "main",
			backendApi,
			dispatch,
			authenticated: true,
			isLoading: false,
			preferredAccount: {
				type: "selfHosted",
				info: {
					username: "alice",
					host: "https://git.example.com",
					viewHost: "https://gitea.example.com",
				},
			},
		});

		expect(gitea.commitUrl("abc123")).toBe(
			"https://gitea.example.com/web/versanis/commit/abc123",
		);
	});

	test("uses account host for separate Gitea SSH host", () => {
		const gitea = new Gitea({
			repo: {
				domain: "third.hostarm.com",
				name: "versanis",
				owner: "web",
				protocol: "ssh:",
			},
			baseBranch: "main",
			backendApi,
			dispatch,
			authenticated: true,
			isLoading: false,
			preferredAccount: {
				type: "selfHosted",
				info: {
					username: "robert.monka",
					host: "https://gitea.hostarm.com",
					viewHost: "https://gitea.hostarm.com",
				},
			},
		});

		expect(gitea.commitUrl("abc123")).toBe(
			"https://gitea.hostarm.com/web/versanis/commit/abc123",
		);
		expect(gitea.branch("feature").url).toBe(
			"https://gitea.hostarm.com/web/versanis/compare/main...feature",
		);
	});
});
