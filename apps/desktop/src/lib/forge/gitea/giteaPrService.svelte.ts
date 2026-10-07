import { MergeMethod, mapForgeReviewToPullRequest } from "$lib/forge/interface/types";
import { invalidatesItem, invalidatesList, providesItem, ReduxTag } from "$lib/state/tags";
import { sleep } from "$lib/utils/sleep";
import { writable } from "svelte/store";
import type { ForgePrService } from "$lib/forge/interface/forgePrService";
import type {
	CreatePullRequestArgs,
	DetailedPullRequest,
	ForgeReview,
	PullRequest,
} from "$lib/forge/interface/types";
import type { BackendApi } from "$lib/state/backendApi";
import type { QueryOptions } from "$lib/state/butlerModule";
import type { PostHogWrapper } from "$lib/telemetry/posthog";
import type { StartQueryActionCreatorOptions } from "@reduxjs/toolkit/query";

export class GiteaPrService implements ForgePrService {
	readonly unit = { name: "Pull request", abbr: "PR", symbol: "#" };
	loading = writable(false);
	private api: ReturnType<typeof injectEndpoints>;

	constructor(
		backendApi: BackendApi,
		private projectId: string,
		private posthog?: PostHogWrapper,
	) {
		this.api = injectEndpoints(backendApi);
	}

	async createPr({
		title,
		body,
		draft,
		baseBranchName,
		upstreamName,
	}: CreatePullRequestArgs): Promise<PullRequest> {
		this.loading.set(true);

		const request = async () => {
			const review = await this.api.endpoints.createGiteaPr.mutate({
				projectId: this.projectId,
				params: {
					title,
					body,
					draft,
					sourceBranch: upstreamName,
					targetBranch: baseBranchName,
				},
			});
			return mapForgeReviewToPullRequest(review);
		};

		let attempts = 0;
		let lastError: unknown;

		try {
			while (attempts < 4) {
				try {
					const response = await request();
					this.posthog?.capture("Gitea PR Successful");
					return response;
				} catch (err: unknown) {
					lastError = err;
					attempts++;
					if (attempts < 4) {
						await sleep(500);
					}
				}
			}
			this.posthog?.capture("Gitea PR Failure");
			throw lastError;
		} finally {
			this.loading.set(false);
		}
	}

	async fetch(number: number, options?: QueryOptions) {
		return await this.api.endpoints.getGiteaPr.fetch(
			{ projectId: this.projectId, reviewId: number },
			options,
		);
	}

	get(number: number, options?: StartQueryActionCreatorOptions) {
		return this.api.endpoints.getGiteaPr.useQuery(
			{ projectId: this.projectId, reviewId: number },
			options,
		);
	}

	async merge(method: MergeMethod, number: number) {
		await this.api.endpoints.mergeGiteaPr.mutate({
			projectId: this.projectId,
			reviewId: number,
			mergeMethod: method,
		});
	}

	async reopen(number: number) {
		await this.api.endpoints.updateGiteaPr.mutate({
			projectId: this.projectId,
			reviewId: number,
			body: null,
			state: "open",
			targetBase: null,
		});
	}

	async update(
		number: number,
		update: { description?: string; state?: "open" | "closed"; targetBase?: string },
	) {
		await this.api.endpoints.updateGiteaPr.mutate({
			projectId: this.projectId,
			reviewId: number,
			body: update.description ?? null,
			state: update.state ?? null,
			targetBase: update.targetBase ?? null,
		});
	}

	async setDraft(projectId: string, reviewId: number, draft: boolean) {
		await this.api.endpoints.setDraftGiteaPr.mutate({ projectId, reviewId, draft });
	}
}

function injectEndpoints(api: BackendApi) {
	return api.injectEndpoints({
		endpoints: (build) => ({
			getGiteaPr: build.query<DetailedPullRequest, { projectId: string; reviewId: number }>({
				extraOptions: {
					command: "get_review",
				},
				query: (args) => args,
				transformResponse: (review: ForgeReview) => reviewToDetailedPullRequest(review),
				providesTags: (_result, _error, args) => providesItem(ReduxTag.PullRequests, args.reviewId),
			}),
			createGiteaPr: build.mutation<
				ForgeReview,
				{
					projectId: string;
					params: {
						title: string;
						body: string;
						sourceBranch: string;
						targetBranch: string;
						draft: boolean;
					};
				}
			>({
				extraOptions: {
					command: "publish_review",
					actionName: "Create Gitea PR",
				},
				query: (args) => args,
				invalidatesTags: [invalidatesList(ReduxTag.PullRequests)],
			}),
			updateGiteaPr: build.mutation<
				void,
				{
					projectId: string;
					reviewId: number;
					body: string | null;
					state: "open" | "closed" | null;
					targetBase: string | null;
				}
			>({
				extraOptions: {
					command: "update_review",
					actionName: "Update Gitea PR",
				},
				query: (args) => args,
				invalidatesTags: [invalidatesList(ReduxTag.PullRequests)],
			}),
			mergeGiteaPr: build.mutation<
				void,
				{ projectId: string; reviewId: number; mergeMethod?: MergeMethod }
			>({
				extraOptions: {
					command: "merge_review",
					actionName: "Merge Gitea PR",
				},
				query: (args) => args,
				invalidatesTags: [invalidatesList(ReduxTag.PullRequests)],
			}),
			setDraftGiteaPr: build.mutation<
				void,
				{ projectId: string; reviewId: number; draft: boolean }
			>({
				extraOptions: {
					command: "set_review_draftiness",
					actionName: "Set Gitea PR Draft State",
				},
				query: (args) => args,
				invalidatesTags: (_result, _error, { reviewId }) => [
					invalidatesItem(ReduxTag.PullRequests, reviewId),
				],
			}),
		}),
	});
}

function reviewToDetailedPullRequest(review: ForgeReview): DetailedPullRequest {
	const pr = mapForgeReviewToPullRequest(review);
	const state = pr.closedAt ? "closed" : "open";
	return {
		id: pr.number,
		title: pr.title,
		author: pr.author,
		body: pr.body,
		number: pr.number,
		sourceBranch: pr.sourceBranch,
		draft: pr.draft,
		fork: false,
		createdAt: pr.createdAt,
		mergedAt: pr.mergedAt,
		closedAt: pr.closedAt,
		updatedAt: pr.modifiedAt,
		htmlUrl: pr.htmlUrl,
		merged: !!pr.mergedAt,
		mergeable: true,
		mergeableState: "unknown",
		rebaseable: true,
		squashable: true,
		state,
		baseBranch: pr.targetBranch,
		reviewers: pr.reviewers.map((reviewer) => ({
			srcUrl: reviewer.srcUrl,
			username: reviewer.name,
		})),
		commentsCount: 0,
		repositorySshUrl: pr.repositorySshUrl,
		repositoryHttpsUrl: pr.repositoryHttpsUrl,
	};
}
