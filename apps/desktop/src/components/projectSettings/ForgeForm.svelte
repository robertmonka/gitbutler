<script lang="ts">
	import BitbucketAccountBadge from "$components/forge/BitbucketAccountBadge.svelte";
	import GitHubAccountBadge from "$components/forge/GitHubAccountBadge.svelte";
	import GiteaAccountBadge from "$components/forge/GiteaAccountBadge.svelte";
	import GitLabAccountBadge from "$components/forge/GitLabAccountBadge.svelte";
	import ForgeAccountConfig from "$components/projectSettings/ForgeAccountConfig.svelte";
	import GitHubOrgRestrictionNotice from "$components/projectSettings/GitHubOrgRestrictionNotice.svelte";
	import { GIT_CONFIG_SERVICE } from "$lib/config/gitConfigService";
	import { isNormalizedError } from "$lib/error/normalizedError";
	import {
		bitbucketAccountIdentifierToString,
		stringToBitbucketAccountIdentifier,
	} from "$lib/forge/bitbucket/bitbucketUserService.svelte";
	import { usePreferredBitbucketUsername } from "$lib/forge/bitbucket/hooks.svelte";
	import GiteaAccountForm from "$components/settings/GiteaAccountForm.svelte";
	import GiteaUserLoginState from "$components/settings/GiteaUserLoginState.svelte";
	import { BASE_BRANCH_SERVICE } from "$lib/baseBranch/baseBranchService.svelte";
	import { FORGE_INFO_SERVICE } from "$lib/forge/forgeInfo.svelte";
	import {
		githubAccountIdentifierToString,
		stringToGitHubAccountIdentifier,
	} from "$lib/forge/github/githubUserService.svelte";
	import { usePreferredGitHubUsername } from "$lib/forge/github/hooks.svelte";
	import {
		giteaAccountIdentifierToString,
		stringToGiteaAccountIdentifier,
	} from "$lib/forge/gitea/giteaUserService.svelte";
	import { usePreferredGiteaUsername } from "$lib/forge/gitea/hooks.svelte";
	import {
		gitlabAccountIdentifierToString,
		stringToGitLabAccountIdentifier,
	} from "$lib/forge/gitlab/gitlabUserService.svelte";
	import { usePreferredGitLabUsername } from "$lib/forge/gitlab/hooks.svelte";
	import { LISTING_SERVICE } from "$lib/forge/listingService.svelte";
	import { PROJECTS_SERVICE } from "$lib/project/projectsService";
	import { inject } from "@gitbutler/core/context";
	import { reactive } from "@gitbutler/shared/reactiveUtils.svelte";
	import { CardGroup, Select, SelectItem, Textbox } from "@gitbutler/ui";

	import type { Project } from "$lib/project/project";
	import type {
		BitbucketAccountIdentifier,
		ForgeName,
		ForgeUser,
		GitHubStackingMode,
		GiteaAccountIdentifier,
		GithubAccountIdentifier,
		GitlabAccountIdentifier,
		ReviewStackingDescription,
	} from "@gitbutler/but-sdk";

	type ForgeSelection = ForgeName | "default";

	const FORGE_OPTIONS: { label: string; value: ForgeSelection }[] = [
		{ label: "None", value: "default" },
		{ label: "GitHub", value: "github" },
		{ label: "GitLab", value: "gitlab" },
		{ label: "Gitea", value: "gitea" },
		{ label: "Azure", value: "azure" },
		{ label: "BitBucket", value: "bitbucket" },
	];

	const { projectId }: { projectId: string } = $props();

	const forgeInfoService = inject(FORGE_INFO_SERVICE);
	const forgeInfoQuery = $derived(forgeInfoService.get(projectId));
	const forgeInfo = $derived(forgeInfoQuery.response);
	const determinedForgeType = $derived(forgeInfo?.name ?? "default");
	const projectsService = inject(PROJECTS_SERVICE);
	const gitConfigService = inject(GIT_CONFIG_SERVICE);
	const gitConfigQuery = $derived(gitConfigService.gbConfig(projectId));
	const reviewStackingDescription = $derived(
		(gitConfigQuery.response?.gitbutlerReviewStackingDescription ??
			"bottom") as ReviewStackingDescription,
	);
	const githubStackingMode = $derived(
		(gitConfigQuery.response?.gitbutlerGithubStackingMode ?? "auto") as GitHubStackingMode,
	);
	const baseBranchService = inject(BASE_BRANCH_SERVICE);
	const projectQuery = $derived(projectsService.getProject(projectId));
	const project = $derived(projectQuery.response);
	const repoInfoQuery = $derived(baseBranchService.repo(projectId));
	const repoDomain = $derived(repoInfoQuery.response?.domain);

	const selectedOption = $derived(project?.forge_override || "default");

	const { preferredGitHubAccount, githubAccounts } = usePreferredGitHubUsername(
		reactive(() => projectId),
	);

	// The workspace's polled review listing keeps this cache entry current,
	// so it reflects whether the integration currently works at all.
	const listingService = inject(LISTING_SERVICE);
	const listingState = $derived(listingService.listingState(projectId));
	const listingError = $derived(listingState.result.error);
	const listingErrorCode = $derived(
		isNormalizedError(listingError) ? listingError.code : undefined,
	);

	// GitLab hooks
	const { preferredGitLabAccount, gitlabAccounts } = usePreferredGitLabUsername(
		reactive(() => projectId),
	);

	// Bitbucket hooks
	const { preferredBitbucketAccount, bitbucketAccounts } = usePreferredBitbucketUsername(
		reactive(() => projectId),
	);
	const { preferredGiteaAccount, giteaAccounts } = usePreferredGiteaUsername(
		reactive(() => projectId),
		reactive(() => repoDomain),
	);

	const resolvedGiteaAccount = $derived(preferredGiteaAccount.current);
	function handleSelectionChange(selectedOption: ForgeSelection) {
		if (!project) return;

		const mutableProject: Project & { unset_forge_override?: boolean } = structuredClone(project);

		if (selectedOption === "default") {
			mutableProject.unset_forge_override = true;
			mutableProject.forge_override = undefined;
		} else {
			mutableProject.forge_override = selectedOption;
		}
		projectsService.updateProject(mutableProject);
	}

	async function updatePreferredForgeUser(projectId: string, forgeUser: ForgeUser) {
		await projectsService.updatePreferredForgeUser(projectId, forgeUser);
		// The cached review listing was fetched with the previous account's
		// credentials; refresh it so a credential-caused failure (e.g. an
		// org OAuth restriction) clears as soon as the account changes
		// instead of on the next 15-minute poll.
		await listingService.refresh(projectId);
	}

	async function updatePreferredGitHubAccount(projectId: string, account: GithubAccountIdentifier) {
		await updatePreferredForgeUser(projectId, { provider: "github", details: account });
	}

	async function updatePreferredGitLabAccount(projectId: string, account: GitlabAccountIdentifier) {
		await updatePreferredForgeUser(projectId, { provider: "gitlab", details: account });
	}

	async function updatePreferredBitbucketAccount(
		projectId: string,
		account: BitbucketAccountIdentifier,
	) {
		await updatePreferredForgeUser(projectId, { provider: "bitbucket", details: account });
	}

	async function updateReviewStackingDescription(value: ReviewStackingDescription) {
		await gitConfigService.setGbConfig(projectId, { gitbutlerReviewStackingDescription: value });
	}

	async function updateGitHubStackingMode(value: GitHubStackingMode) {
		await gitConfigService.setGbConfig(projectId, { gitbutlerGithubStackingMode: value });
	}

	function updatePreferredGiteaAccount(projectId: string, account: GiteaAccountIdentifier) {
		projectsService.updatePreferredForgeUser(projectId, {
			provider: "gitea",
			details: account,
		});
	}

	// Pin the resolved global account to this project when none is stored yet.
	$effect(() => {
		const account = resolvedGiteaAccount;
		const proj = project;
		if (account === undefined || proj === undefined) return;
		if (proj.preferred_forge_user !== null) return;
		updatePreferredGiteaAccount(projectId, account);
	});
</script>

<CardGroup>
	<CardGroup.Item>
		{#snippet title()}
			Forge override
		{/snippet}

		{#snippet caption()}
			{#if determinedForgeType === "default"}
				We couldn't detect which Forge you're using.
				<br />
				To enable Forge integration, please select your Forge from the dropdown below.
				<br />
				<span class="text-bold">Note:</span> Currently, only GitHub, GitLab and Bitbucket support pull
				request creation.
				<span class="text-bold">Note:</span> GitHub, GitLab, and Gitea support review creation.
			{:else}
				We’ve detected that you’re using <span class="text-bold"
					>{determinedForgeType.toUpperCase()}</span
				>.
				<br />
				At the moment, it’s not possible to manually override the detected forge type.
			{/if}
		{/snippet}

		{#if determinedForgeType === "default"}
			<Select
				value={selectedOption}
				options={FORGE_OPTIONS}
				wide
				onselect={(value) => handleSelectionChange(value as ForgeSelection)}
			>
				{#snippet itemSnippet({ item, highlighted })}
					<SelectItem selected={item.value === selectedOption} {highlighted}>
						{item.label}
					</SelectItem>
				{/snippet}
			</Select>
		{/if}
	</CardGroup.Item>

	<CardGroup.Item>
		{#snippet title()}
			Stack information in review descriptions
		{/snippet}

		{#snippet caption()}
			Choose where GitButler-managed stack information appears. Changes apply on the next review
			sync. The default is Bottom. Does not apply to native GitHub stacked pull requests, where
			GitHub shows the stack on its own.
		{/snippet}

		<div data-testid="review-stacking-description-select">
			<Select
				value={reviewStackingDescription}
				options={[
					{ label: "Bottom", value: "bottom" },
					{ label: "Top", value: "top" },
					{ label: "Disabled", value: "disabled" },
				]}
				wide
				onselect={(value) => updateReviewStackingDescription(value as ReviewStackingDescription)}
			>
				{#snippet itemSnippet({ item, highlighted })}
					<div data-testid={`review-stacking-description-option-${item.value}`}>
						<SelectItem selected={item.value === reviewStackingDescription} {highlighted}>
							{item.label}
						</SelectItem>
					</div>
				{/snippet}
			</Select>
		</div>
	</CardGroup.Item>

	{#if forgeInfo?.name === "github"}
		<CardGroup.Item>
			{#snippet title()}
				Native GitHub stacked pull requests
			{/snippet}

			{#snippet caption()}
				Register this project’s reviewed stacks with GitHub’s private-preview stacks API. Higher
				pull requests may merge the pull requests below them. Auto falls back to description
				metadata when the repository is not enrolled in the preview, while Native reports an error.
				Changes apply on the next push or pull request creation. Fork-backed pull requests always
				use description metadata.
			{/snippet}

			<div data-testid="github-stacking-mode-select">
				<Select
					value={githubStackingMode}
					options={[
						{ label: "Auto", value: "auto" },
						{ label: "Disabled", value: "disabled" },
						{ label: "Native", value: "native" },
					]}
					wide
					onselect={(value) => updateGitHubStackingMode(value as GitHubStackingMode)}
				>
					{#snippet itemSnippet({ item, highlighted })}
						<div data-testid={`github-stacking-mode-option-${item.value}`}>
							<SelectItem selected={item.value === githubStackingMode} {highlighted}>
								{item.label}
							</SelectItem>
						</div>
					{/snippet}
				</Select>
			</div>
		</CardGroup.Item>

		<ForgeAccountConfig
			{projectId}
			displayName="GitHub"
			accounts={githubAccounts.current}
			preferredAccount={preferredGitHubAccount.current}
			accountToString={githubAccountIdentifierToString}
			stringToAccount={stringToGitHubAccountIdentifier}
			getUsername={(account) => account.info.username}
			updatePreferredAccount={updatePreferredGitHubAccount}
			AccountBadge={GitHubAccountBadge}
			docsUrl="https://docs.gitbutler.com/features/forge-integration/github-integration"
			requestType="pull request"
		>
			{#snippet notice()}
				<GitHubOrgRestrictionNotice errorCode={listingErrorCode} />
			{/snippet}
		</ForgeAccountConfig>
	{/if}

	{#if forgeInfo?.name === "gitlab"}
		<ForgeAccountConfig
			{projectId}
			displayName="GitLab"
			accounts={gitlabAccounts.current}
			preferredAccount={preferredGitLabAccount.current}
			accountToString={gitlabAccountIdentifierToString}
			stringToAccount={stringToGitLabAccountIdentifier}
			getUsername={(account) => account.info.username}
			updatePreferredAccount={updatePreferredGitLabAccount}
			AccountBadge={GitLabAccountBadge}
			docsUrl="https://docs.gitbutler.com/features/forge-integration/gitlab-integration"
			requestType="merge request"
		/>
	{/if}

	{#if forgeInfo?.name === "bitbucket"}
		<ForgeAccountConfig
			{projectId}
			displayName="Bitbucket"
			accounts={bitbucketAccounts.current}
			preferredAccount={preferredBitbucketAccount.current}
			accountToString={bitbucketAccountIdentifierToString}
			stringToAccount={stringToBitbucketAccountIdentifier}
			getUsername={(account) => account.info.email}
			updatePreferredAccount={updatePreferredBitbucketAccount}
			AccountBadge={BitbucketAccountBadge}
			docsUrl="https://docs.gitbutler.com/features/forge-integration/bitbucket-integration"
			requestType="pull request"
		/>
	{/if}

	{#if forgeInfo?.name === "gitea"}
		<ForgeAccountConfig
			{projectId}
			displayName="Gitea"
			accounts={giteaAccounts.current}
			preferredAccount={resolvedGiteaAccount}
			accountToString={giteaAccountIdentifierToString}
			stringToAccount={stringToGiteaAccountIdentifier}
			getUsername={(account) => account.info.username}
			updatePreferredAccount={updatePreferredGiteaAccount}
			AccountBadge={GiteaAccountBadge}
			docsUrl="https://docs.gitbutler.com/features/forge-integration/gitea-integration"
			requestType="pull request"
		/>

		<CardGroup.Item>
			{#snippet title()}
				Gitea instance
			{/snippet}

			{#snippet caption()}
				{#if resolvedGiteaAccount}
					Account connected globally. URLs below are used for API calls and opening the web UI.
				{:else}
					Add a Gitea account in General Settings or connect one below.
				{/if}
			{/snippet}

			{#if resolvedGiteaAccount}
				<GiteaUserLoginState account={resolvedGiteaAccount} />
				{#if resolvedGiteaAccount.type === "selfHosted"}
					<div class="gitea-instance-urls">
						<Textbox
							label="API URL"
							size="large"
							value={resolvedGiteaAccount.info.host}
							readonly
							helperText="Used for authentication and pull requests"
						/>
						<Textbox
							label="View URL"
							size="large"
							value={resolvedGiteaAccount.info.viewHost ?? resolvedGiteaAccount.info.host}
							readonly
							helperText="Opened in the browser for this project"
						/>
					</div>
				{/if}
			{:else}
				<GiteaAccountForm
					submitLabel="Add account"
					onStored={(account) => updatePreferredGiteaAccount(projectId, account)}
				/>
			{/if}
		</CardGroup.Item>
	{/if}
</CardGroup>

<style lang="postcss">
	.gitea-instance-urls {
		display: flex;
		flex-direction: column;
		gap: 12px;
		margin-top: 12px;
	}
</style>
