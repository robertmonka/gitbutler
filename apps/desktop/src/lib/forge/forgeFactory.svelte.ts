import { AZURE_DOMAIN, AzureDevOps } from "$lib/forge/azure/azure";
import { BitBucket, BITBUCKET_DOMAIN } from "$lib/forge/bitbucket/bitbucket";
import { DefaultForge } from "$lib/forge/default/default";
import { Gitea, GITEA_DOMAIN } from "$lib/forge/gitea/gitea";
import { giteaAccountMatchesRepoDomain } from "$lib/forge/gitea/giteaUserService.svelte";
import { GitHub, GITHUB_DOMAIN } from "$lib/forge/github/github";
import { GitHubClient } from "$lib/forge/github/githubClient";
import { GitLab, GITLAB_DOMAIN, GITLAB_SUB_DOMAIN } from "$lib/forge/gitlab/gitlab";
import { InjectionToken } from "@gitbutler/core/context";
import { deepCompare } from "@gitbutler/shared/compare";
import type { ForgeProvider } from "$lib/baseBranch/baseBranch";
import type { GitLabClient } from "$lib/forge/gitlab/gitlabClient.svelte";
import type { Forge, ForgeName } from "$lib/forge/interface/forge";
import type { RepoInfo } from "$lib/git/gitUrl";
import type { BackendApi } from "$lib/state/backendApi";
import type { AppDispatch, GitHubApi, GitLabApi } from "$lib/state/clientState.svelte";
import type { ReduxTag } from "$lib/state/tags";
import type { PostHogWrapper } from "$lib/telemetry/posthog";
import type { Code, GiteaAccountIdentifier } from "@gitbutler/but-sdk";
import type { Reactive } from "@gitbutler/shared/storeUtils";
import type { TagDescription } from "@reduxjs/toolkit/query";

type ForgeAuthError = { code?: Code; message: string };

export type ForgeConfig = {
	repo?: RepoInfo;
	pushRepo?: RepoInfo;
	baseBranch?: string;
	githubAuthenticated?: boolean;
	forgeIsLoading?: boolean;
	githubError?: ForgeAuthError;
	gitlabAuthenticated?: boolean;
	giteaAuthenticated?: boolean;
	giteaError?: ForgeAuthError;
	giteaPreferredAccount?: GiteaAccountIdentifier;
	detectedForgeProvider: ForgeProvider | undefined;
	forgeOverride?: ForgeName;
	projectId?: string;
};

export const DEFAULT_FORGE_FACTORY = new InjectionToken<DefaultForgeFactory>("DefaultForgeFactory");

export class DefaultForgeFactory implements Reactive<Forge> {
	private default = new DefaultForge();
	private _forge = $state<Forge>(this.default);
	private _config: any = undefined;
	private _determinedForgeType = $state<ForgeName>("default");
	private _githubError = $state<ForgeAuthError | undefined>(undefined);
	private _giteaError = $state<ForgeAuthError | undefined>(undefined);
	private _canSetupIntegration = $derived.by(() => {
		// Don't show the setup prompt if there's a network error
		if (this.currentForgeAuthError?.code === "NetworkError") {
			return undefined;
		}
		return isAvalilableForge(this._determinedForgeType) &&
			!this._forge.authenticated &&
			!this._forge.isLoading
			? this._determinedForgeType
			: undefined;
	});

	constructor(
		private params: {
			backendApi: BackendApi;
			gitHubClient: GitHubClient;
			gitHubApi: GitHubApi;
			gitLabClient: GitLabClient;
			gitLabApi: GitLabApi;
			posthog: PostHogWrapper;
			dispatch: AppDispatch;
		},
	) {}

	get current(): Forge {
		return this._forge;
	}

	get determinedForgeType(): ForgeName {
		return this._determinedForgeType;
	}

	get canSetupIntegration(): AvailableForge | undefined {
		return this._canSetupIntegration;
	}

	private get currentForgeAuthError(): ForgeAuthError | undefined {
		switch (this._determinedForgeType) {
			case "github":
				return this._githubError;
			case "gitea":
				return this._giteaError;
			default:
				return undefined;
		}
	}

	/**
	 * Get review unit abbreviation with fallback to 'PR'
	 */
	get reviewUnitAbbr(): string {
		return this._forge.prService?.unit.abbr ?? "PR";
	}

	/**
	 * Get review unit name with fallback to 'Pull request'
	 */
	get reviewUnitName(): string {
		return this._forge.prService?.unit.name ?? "Pull request";
	}

	/**
	 * Get review unit symbol with fallback to '#'
	 */
	get reviewUnitSymbol(): string {
		return this._forge.prService?.unit.symbol ?? "#";
	}

	setConfig(config: ForgeConfig) {
		if (deepCompare(config, this._config)) {
			return;
		}
		this._config = config;
		const {
			repo,
			pushRepo,
			baseBranch,
			githubAuthenticated,
			forgeIsLoading,
			githubError,
			gitlabAuthenticated,
			giteaAuthenticated,
			giteaError,
			giteaPreferredAccount,
			detectedForgeProvider,
			forgeOverride,
			projectId,
		} = config;
		this._githubError = githubError;
		this._giteaError = giteaError;
		if (repo && baseBranch) {
			this._determinedForgeType = this.determineForgeType(
				repo,
				detectedForgeProvider,
				giteaPreferredAccount,
			);
			this._forge = this.build({
				repo,
				pushRepo,
				baseBranch,
				githubAuthenticated,
				forgeIsLoading,
				gitlabAuthenticated,
				detectedForgeProvider,
				forgeOverride,
				giteaAuthenticated,
				giteaPreferredAccount,
				projectId,
			});
		} else {
			this._determinedForgeType = "default";
			this._forge = this.default;
		}
	}

	build({
		repo,
		pushRepo,
		baseBranch,
		githubAuthenticated,
		forgeIsLoading,
		gitlabAuthenticated,
		detectedForgeProvider,
		forgeOverride,
		giteaAuthenticated,
		giteaPreferredAccount,
		projectId,
	}: {
		repo: RepoInfo;
		pushRepo?: RepoInfo;
		baseBranch: string;
		githubAuthenticated?: boolean;
		forgeIsLoading?: boolean;
		gitlabAuthenticated?: boolean;
		detectedForgeProvider: ForgeProvider | undefined;
		forgeOverride: ForgeName | undefined;
		giteaAuthenticated?: boolean;
		giteaPreferredAccount?: GiteaAccountIdentifier;
		projectId?: string;
	}): Forge {
		let forgeType = this.determineForgeType(repo, detectedForgeProvider, giteaPreferredAccount);
		if (forgeType === "default" && forgeOverride) {
			forgeType = forgeOverride;
		}
		const forkStr =
			pushRepo && pushRepo.hash !== repo.hash ? `${pushRepo.owner}:${pushRepo.name}` : undefined;

		const baseParams = {
			repo,
			baseBranch,
			forkStr,
			authenticated: false,
		};

		if (forgeType === "github") {
			const { gitHubClient, gitHubApi, posthog, backendApi, dispatch } = this.params;
			return new GitHub({
				...baseParams,
				dispatch,
				api: gitHubApi,
				backendApi,
				client: gitHubClient,
				posthog: posthog,
				authenticated: !!githubAuthenticated,
				isLoading: forgeIsLoading ?? false,
			});
		}
		if (forgeType === "gitlab") {
			const { gitLabClient, gitLabApi, posthog, dispatch, backendApi } = this.params;
			return new GitLab({
				...baseParams,
				api: gitLabApi,
				backendApi,
				client: gitLabClient,
				posthog: posthog,
				authenticated: !!gitlabAuthenticated,
				dispatch,
				isLoading: forgeIsLoading ?? false,
			});
		}
		if (forgeType === "gitea") {
			const { posthog, dispatch, backendApi } = this.params;
			return new Gitea({
				...baseParams,
				projectId,
				backendApi,
				posthog: posthog,
				authenticated: !!giteaAuthenticated,
				preferredAccount: giteaPreferredAccount,
				dispatch,
				isLoading: forgeIsLoading ?? false,
			});
		}
		if (forgeType === "bitbucket") {
			return new BitBucket(baseParams);
		}
		if (forgeType === "azure") {
			return new AzureDevOps(baseParams);
		}
		return this.default;
	}

	private determineForgeType(
		repo: RepoInfo,
		detectedForgeProvider: ForgeProvider | undefined,
		giteaPreferredAccount?: GiteaAccountIdentifier,
	): ForgeName {
		const domain = repo.domain;

		if (
			giteaPreferredAccount?.type === "selfHosted" &&
			giteaAccountMatchesRepoDomain(giteaPreferredAccount, domain)
		) {
			return "gitea";
		}

		if (detectedForgeProvider) {
			return detectedForgeProvider;
		}

		if (domain.includes(GITHUB_DOMAIN)) {
			return "github";
		}
		if (
			domain === GITLAB_DOMAIN ||
			domain.startsWith(GITLAB_SUB_DOMAIN + ".") ||
			domain.startsWith("xy" + GITLAB_SUB_DOMAIN + ".") // Temporary workaround until we have foerge overrides implemented
		) {
			return "gitlab";
		}
		if (domain.includes(GITEA_DOMAIN)) {
			return "gitea";
		}
		if (domain.includes(BITBUCKET_DOMAIN)) {
			return "bitbucket";
		}
		if (domain.includes(AZURE_DOMAIN)) {
			return "azure";
		}

		return "default";
	}

	invalidate(tags: TagDescription<ReduxTag>[]) {
		const action = this.current.invalidate(tags);
		const { dispatch } = this.params;
		if (action) {
			dispatch(action);
		}
	}
}

const AVAILABLE_FORGES = ["github", "gitlab", "gitea"] satisfies ForgeName[];
export type AvailableForge = (typeof AVAILABLE_FORGES)[number];

function isAvalilableForge(forge: ForgeName): forge is AvailableForge {
	return AVAILABLE_FORGES.includes(forge as AvailableForge);
}

export function availableForgeLabel(forge: AvailableForge): string {
	switch (forge) {
		case "github":
			return "GitHub";
		case "gitlab":
			return "GitLab";
		case "gitea":
			return "Gitea";
	}
}

export function availableForgeReviewUnit(forge: AvailableForge): string {
	switch (forge) {
		case "github":
			return "Pull Requests";
		case "gitlab":
			return "Merge Requests";
		case "gitea":
			return "Pull Requests";
	}
}

export function availableForgeDocsLink(forge: AvailableForge): string {
	switch (forge) {
		case "github":
			return "https://docs.gitbutler.com/features/forge-integration/github-integration";
		case "gitlab":
			return "https://docs.gitbutler.com/features/forge-integration/gitlab-integration";
		case "gitea":
			return "https://docs.gitbutler.com/features/forge-integration/gitea-integration";
	}
}
