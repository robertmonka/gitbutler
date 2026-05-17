<script lang="ts">
	import GiteaAccountForm from "$components/settings/GiteaAccountForm.svelte";
	import GiteaUserLoginState from "$components/settings/GiteaUserLoginState.svelte";
	import ReduxResult from "$components/shared/ReduxResult.svelte";
	import giteaLogoSvg from "$lib/assets/unsized-logos/gitea.svg?raw";
	import { GITEA_USER_SERVICE } from "$lib/forge/gitea/giteaUserService.svelte";
	import { inject } from "@gitbutler/core/context";

	import { AddForgeAccountButton, Button, CardGroup } from "@gitbutler/ui";
	import { fade } from "svelte/transition";

	const giteaUserService = inject(GITEA_USER_SERVICE);

	const [clearAll, clearingAllResult] = giteaUserService.deleteAllGiteaAccounts();
	const accounts = giteaUserService.accounts();

	let showingFlow = $state(false);

	function cleanupSelfHostedFlow() {
		showingFlow = false;
	}

	async function deleteAllGiteaAccounts() {
		await clearAll();
		showingFlow = true;
	}
</script>

<div class="stack-v gap-8">
	<CardGroup>
		<ReduxResult result={accounts.result}>
			{#snippet error()}
				<CardGroup.Item>
					{#snippet title()}
						Failed to load Gitea accounts
					{/snippet}
					<Button
						style="pop"
						onclick={deleteAllGiteaAccounts}
						loading={clearingAllResult.current.isLoading}>Try again</Button
					>
				</CardGroup.Item>
			{/snippet}

			{#snippet children(accounts)}
				{@const noAccounts = accounts.length === 0}
				{#each accounts as account}
					<GiteaUserLoginState {account} />
				{/each}

				<CardGroup.Item background={accounts.length > 0 ? "var(--bg-2)" : undefined}>
					{#snippet iconSide()}
						<div class="icon-wrapper__logo">
							{@html giteaLogoSvg}
						</div>
					{/snippet}

					{#snippet title()}
						Gitea
					{/snippet}

					{#snippet caption()}
						Allows you to create Pull Requests
					{/snippet}

					{#snippet actions()}
						{@render addProfileButton(noAccounts)}
					{/snippet}
				</CardGroup.Item>
			{/snippet}
		</ReduxResult>
	</CardGroup>

	{#if showingFlow}
		<div in:fade={{ duration: 100 }}>
			<CardGroup>
				<CardGroup.Item>
					{#snippet title()}
						Add Gitea Account
					{/snippet}

					<GiteaAccountForm onCancel={cleanupSelfHostedFlow} onStored={cleanupSelfHostedFlow} />
				</CardGroup.Item>
			</CardGroup>
		</div>
	{/if}
</div>

<p class="text-12 text-body gitea-integration-settings__text">
	Credentials are persisted locally in your OS Keychain / Credential Manager.
</p>

{#snippet addProfileButton(noAccounts: boolean)}
	<AddForgeAccountButton
		{noAccounts}
		disabled={showingFlow}
		menuItems={[
			{
				label: "Add Gitea Account",
				icon: "factory",
				onclick: () => (showingFlow = true),
			},
		]}
	/>
{/snippet}

<style lang="postcss">
	.icon-wrapper__logo {
		width: 28px;
		height: 28px;
	}

	.gitea-integration-settings__text {
		color: var(--text-2);
	}
</style>
