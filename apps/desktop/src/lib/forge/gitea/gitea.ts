import { GiteaBranch } from "$lib/forge/gitea/giteaBranch";
import { GiteaListingService } from "$lib/forge/gitea/giteaListingService.svelte";
import { GiteaPrService } from "$lib/forge/gitea/giteaPrService.svelte";
import { providesList, ReduxTag } from "$lib/state/tags";
import type { Forge, ForgeName } from "$lib/forge/interface/forge";
import type { ForgeArguments, ForgeUser } from "$lib/forge/interface/types";
import type { BackendApi } from "$lib/state/backendApi";
import type { AppDispatch } from "$lib/state/clientState.svelte";
import type { PostHogWrapper } from "$lib/telemetry/posthog";
import type { GiteaAccountIdentifier, GiteaAuthenticatedUserSensitive } from "@gitbutler/but-sdk";
import type { ReactiveQuery } from "$lib/state/butlerModule";
import type { TagDescription } from "@reduxjs/toolkit/query";

export const GITEA_DOMAIN = "gitea";

export class Gitea implements Forge {
	readonly name: ForgeName = "gitea";
	readonly authenticated: boolean;
	readonly isLoading: boolean;
	private baseUrl: string;
	private baseBranch: string;
	private forkStr?: string;
	private api: ReturnType<typeof injectEndpoints>;

	constructor(
		private params: ForgeArguments & {
			projectId?: string;
			posthog?: PostHogWrapper;
			backendApi: BackendApi;
			dispatch: AppDispatch;
			isLoading: boolean;
			preferredAccount?: GiteaAccountIdentifier;
		},
	) {
		const { backendApi, baseBranch, forkStr, authenticated, repo, isLoading, preferredAccount } =
			this.params;
		let protocol = repo.protocol?.endsWith(":")
			? repo.protocol.slice(0, -1)
			: repo.protocol || "https";

		if (protocol === "ssh") {
			protocol = "https";
		}

		const baseHost =
			preferredAccount?.type === "selfHosted"
				? (preferredAccount.info.viewHost ?? preferredAccount.info.host).replace(/\/$/, "")
				: `${protocol}://${repo.domain}`;
		this.baseUrl = `${baseHost}/${repo.owner}/${repo.name}`;
		this.baseBranch = baseBranch;
		this.forkStr = forkStr;
		this.authenticated = authenticated;
		this.isLoading = isLoading;
		this.api = injectEndpoints(backendApi);
	}

	branch(name: string) {
		return new GiteaBranch(name, this.baseBranch, this.baseUrl, this.forkStr);
	}

	commitUrl(id: string): string {
		return `${this.baseUrl}/commit/${id}`;
	}

	get user(): ReactiveQuery<ForgeUser> {
		const { preferredAccount } = this.params;
		if (!preferredAccount) {
			return {
				result: { status: "uninitialized" as const, data: undefined },
			} as ReactiveQuery<ForgeUser>;
		}
		return this.api.endpoints.getGiteaForgeUser.useQuery({ account: preferredAccount });
	}

	get listService() {
		if (!this.authenticated) return;
		const { backendApi, dispatch } = this.params;
		return new GiteaListingService(backendApi, dispatch);
	}

	get issueService() {
		return undefined;
	}

	get prService() {
		const { backendApi, posthog, projectId } = this.params;
		if (!this.authenticated || !projectId) return;
		return new GiteaPrService(backendApi, projectId, posthog);
	}

	get repoService() {
		return undefined;
	}

	get checks() {
		return undefined;
	}

	invalidate(tags: TagDescription<ReduxTag>[]) {
		return this.params.backendApi.util.invalidateTags(tags);
	}
}

function injectEndpoints(api: BackendApi) {
	return api.injectEndpoints({
		endpoints: (build) => ({
			getGiteaForgeUser: build.query<ForgeUser, { account: GiteaAccountIdentifier }>({
				extraOptions: {
					command: "get_gitea_user",
				},
				query: (args) => args,
				transformResponse: (user: GiteaAuthenticatedUserSensitive | null) => ({
					id: 0,
					name: user?.name ?? user?.username ?? "",
					srcUrl: user?.avatarUrl ?? "",
				}),
				providesTags: [providesList(ReduxTag.ForgeUser)],
			}),
		}),
	});
}
