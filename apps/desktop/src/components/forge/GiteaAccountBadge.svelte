<script lang="ts">
	import { Badge } from "@gitbutler/ui";
	import type { GiteaAccountIdentifier } from "@gitbutler/but-sdk";

	type Props = {
		account: GiteaAccountIdentifier;
		class?: string;
	};

	const { account, class: className }: Props = $props();

	export function badgeText(account: GiteaAccountIdentifier): string | null {
		switch (account.type) {
			case "selfHosted":
				return account.info.host;
		}
	}

	export function tooltipText(account: GiteaAccountIdentifier): string {
		switch (account.type) {
			case "selfHosted":
				return "Self-Hosted Gitea";
		}
	}
</script>

<Badge class={className} tooltip={tooltipText(account)}>
	{badgeText(account)}
</Badge>
