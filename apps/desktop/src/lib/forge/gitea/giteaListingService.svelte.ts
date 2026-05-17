import { createSelectByIds } from "$lib/state/customSelectors";
import { invalidatesList, providesList, ReduxTag } from "$lib/state/tags";
import { isDefined } from "@gitbutler/ui/utils/typeguards";
import { createEntityAdapter, type EntityState } from "@reduxjs/toolkit";
import type { ForgeListingService } from "$lib/forge/interface/forgeListingService";
import {
	mapForgeReviewToPullRequest,
	type ForgeReview,
	type PullRequest,
} from "$lib/forge/interface/types";
import type { BackendApi } from "$lib/state/backendApi";
import type { AppDispatch } from "$lib/state/clientState.svelte";

export class GiteaListingService implements ForgeListingService {
	private api: ReturnType<typeof injectEndpoints>;

	constructor(
		backendApi: BackendApi,
		private readonly dispatch: AppDispatch,
	) {
		this.api = injectEndpoints(backendApi);
	}

	list(projectId: string, pollingInterval?: number) {
		return this.api.endpoints.listGiteaPrs.useQuery(projectId, {
			transform: (result) => prSelectors.selectAll(result),
			subscriptionOptions: { pollingInterval },
		});
	}

	getByBranch(projectId: string, branchName: string) {
		return this.api.endpoints.listGiteaPrs.useQuery(projectId, {
			transform: (result) => prSelectors.selectById(result, branchName),
		});
	}

	filterByBranch(projectId: string, branchName: string[]) {
		return this.api.endpoints.listGiteaPrs.useQueryState(projectId, {
			transform: (result) => prSelectors.selectByIds(result, branchName),
		});
	}

	async fetchByBranch(projectId: string, branchNames: string[]) {
		const result = await this.api.endpoints.listGiteaPrs.fetch(projectId);
		return branchNames
			.map((branch) => result && prSelectors.selectById(result, branch))
			.filter(isDefined);
	}

	async refresh(_projectId: string): Promise<void> {
		this.dispatch(this.api.util.invalidateTags([invalidatesList(ReduxTag.PullRequests)]));
	}
}

function injectEndpoints(api: BackendApi) {
	return api.injectEndpoints({
		endpoints: (build) => ({
			listGiteaPrs: build.query<EntityState<PullRequest, string>, string>({
				extraOptions: {
					command: "list_reviews",
				},
				query: (projectId) => ({ projectId }),
				transformResponse: (reviews: ForgeReview[]) =>
					prAdapter.addMany(
						prAdapter.getInitialState(),
						reviews.map((review) => mapForgeReviewToPullRequest(review)),
					),
				providesTags: [providesList(ReduxTag.PullRequests)],
			}),
		}),
	});
}

const prAdapter = createEntityAdapter<PullRequest, string>({
	selectId: (pr) => pr.sourceBranch,
});

const prSelectors = { ...prAdapter.getSelectors(), selectByIds: createSelectByIds<PullRequest>() };
