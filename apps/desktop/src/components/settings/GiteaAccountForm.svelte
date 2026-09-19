<script lang="ts">
	import { GITEA_USER_SERVICE } from "$lib/forge/gitea/giteaUserService.svelte";
	import { inject } from "@gitbutler/core/context";
	import { Button, Textbox } from "@gitbutler/ui";
	import type { GiteaAccountIdentifier } from "@gitbutler/but-sdk";

	type Props = {
		initialApiHost?: string;
		initialViewHost?: string;
		submitLabel?: string;
		onCancel?: () => void;
		onStored?: (account: GiteaAccountIdentifier) => void;
	};

	const {
		initialApiHost,
		initialViewHost,
		submitLabel = "Add account",
		onCancel,
		onStored,
	}: Props = $props();

	const giteaUserService = inject(GITEA_USER_SERVICE);
	const [storeSelfHostedPat, storeSelfHostedPatResult] = giteaUserService.storeGiteaSelfHostedPat;

	let selfHostedPatInput = $state<string>();
	let selfHostedApiHostInput = $state<string | undefined>();
	let selfHostedViewHostInput = $state<string | undefined>();
	let selfHostedPatError = $state<string>();
	let selfHostedApiHostError = $state<string>();
	let selfHostedViewHostError = $state<string>();
	let appliedInitialApiHost = $state<string | undefined>();
	let appliedInitialViewHost = $state<string | undefined>();

	$effect(() => {
		if (
			initialApiHost &&
			initialApiHost !== appliedInitialApiHost &&
			(!selfHostedApiHostInput || selfHostedApiHostInput === appliedInitialApiHost)
		) {
			selfHostedApiHostInput = initialApiHost;
			appliedInitialApiHost = initialApiHost;
		}
		if (
			initialViewHost &&
			initialViewHost !== appliedInitialViewHost &&
			(!selfHostedViewHostInput || selfHostedViewHostInput === appliedInitialViewHost)
		) {
			selfHostedViewHostInput = initialViewHost;
			appliedInitialViewHost = initialViewHost;
		}
	});

	function cleanupSelfHostedFlow() {
		selfHostedPatInput = undefined;
		selfHostedApiHostInput = initialApiHost;
		selfHostedViewHostInput = initialViewHost;
		selfHostedPatError = undefined;
		selfHostedApiHostError = undefined;
		selfHostedViewHostError = undefined;
	}

	async function storeSelfHostedToken() {
		if (!selfHostedPatInput || !selfHostedApiHostInput) return;
		selfHostedPatError = undefined;
		selfHostedApiHostError = undefined;
		selfHostedViewHostError = undefined;
		const viewHost = selfHostedViewHostInput?.trim();
		try {
			const account = await storeSelfHostedPat({
				accessToken: selfHostedPatInput,
				host: selfHostedApiHostInput,
				viewHost: viewHost || undefined,
			});
			onStored?.({
				type: "selfHosted",
				info: {
					host: account.host,
					viewHost: account.viewHost ?? undefined,
					username: account.username,
				},
			});
			cleanupSelfHostedFlow();
		} catch (err: any) {
			console.error("Failed to store Gitea PAT:", err);
			selfHostedPatError = "Invalid token or host";
		}
	}
</script>

<Textbox
	label="API URL"
	size="large"
	value={selfHostedApiHostInput}
	oninput={(value) => (selfHostedApiHostInput = value)}
	helperText="Gitea API base URL used for authentication and pull requests"
	error={selfHostedApiHostError}
/>
<Textbox
	label="View URL"
	size="large"
	value={selfHostedViewHostInput}
	oninput={(value) => (selfHostedViewHostInput = value)}
	helperText="Web UI URL opened in the browser. Leave empty when it matches the API URL."
	error={selfHostedViewHostError}
/>
<Textbox
	label="Personal Access Token"
	size="large"
	type="password"
	value={selfHostedPatInput}
	oninput={(value) => (selfHostedPatInput = value)}
	error={selfHostedPatError}
/>

<div class="flex justify-end gap-6">
	{#if onCancel}
		<Button style="gray" kind="outline" onclick={onCancel}>Cancel</Button>
	{/if}
	<Button
		style="pop"
		disabled={!selfHostedApiHostInput || !selfHostedPatInput}
		loading={storeSelfHostedPatResult.current.isLoading}
		onclick={storeSelfHostedToken}
	>
		{submitLabel}
	</Button>
</div>
