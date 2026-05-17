import {
	GITEA_USER_SERVICE,
	giteaAccountMatchesRepoDomain,
	isSameGiteaAccountIdentifier,
} from "$lib/forge/gitea/giteaUserService.svelte";
import { PROJECTS_SERVICE } from "$lib/project/projectsService";
import { inject } from "@gitbutler/core/context";
import { reactive } from "@gitbutler/shared/reactiveUtils.svelte";
import type { Code, GiteaAccountIdentifier } from "@gitbutler/but-sdk";
import type { ForgeUserQuery } from "$lib/forge/interface/types";
import type { Reactive } from "@gitbutler/shared/storeUtils";

type GiteaPreferences = {
	preferredGiteaAccount: Reactive<GiteaAccountIdentifier | undefined>;
	giteaAccounts: Reactive<GiteaAccountIdentifier[]>;
};

export function usePreferredGiteaUsername(
	projectId: Reactive<string>,
	repoDomain: Reactive<string | undefined> = reactive(() => undefined),
): GiteaPreferences {
	const giteaUserService = inject(GITEA_USER_SERVICE);
	const projectsService = inject(PROJECTS_SERVICE);
	const giteaAccountsResponse = giteaUserService.accounts();
	const giteaAccounts = $derived(giteaAccountsResponse?.response ?? []);

	const projectQuery = $derived(projectsService.getProject(projectId.current));
	const project = $derived(projectQuery.response);
	const preferredUser = $derived.by(() => {
		if (giteaAccounts.length === 0) {
			return undefined;
		}

		const domain = repoDomain.current;
		const matchByDomain = () =>
			giteaAccounts.find((account) => giteaAccountMatchesRepoDomain(account, domain));

		// Match GitHub/GitLab: global login is enough; project preference overrides when set.
		if (
			project === undefined ||
			project.preferred_forge_user === null ||
			project.preferred_forge_user.provider !== "gitea"
		) {
			return matchByDomain() ?? giteaAccounts.at(0);
		}

		const preferredForgeUser = project.preferred_forge_user.details;
		return (
			giteaAccounts.find((account) =>
				isSameGiteaAccountIdentifier(account, preferredForgeUser),
			) ??
			matchByDomain() ??
			giteaAccounts.at(0)
		);
	});

	return {
		preferredGiteaAccount: reactive(() => preferredUser),
		giteaAccounts: reactive(() => giteaAccounts),
	};
}

export function useGiteaForgeUser(projectId: Reactive<string>): ForgeUserQuery {
	const giteaUserService = inject(GITEA_USER_SERVICE);
	const { preferredGiteaAccount } = usePreferredGiteaUsername(projectId);
	const userQuery = $derived.by(() => {
		const account = preferredGiteaAccount.current;
		if (account === undefined) return undefined;
		return giteaUserService.authenticatedUser(account);
	});
	return {
		user: reactive(() => {
			const result = userQuery?.response;
			if (result === undefined || result === null) return undefined;
			return {
				login: result.username,
				name: result.name ?? result.username,
				srcUrl: result.avatarUrl ?? "",
			};
		}),
		isLoading: reactive(() => userQuery?.result.isLoading ?? false),
	};
}

type GiteaAccess = {
	host: Reactive<string | undefined>;
	accessToken: Reactive<string | undefined>;
	isLoading: Reactive<boolean>;
	error: Reactive<{ code?: Code; message: string } | undefined>;
	isError: Reactive<boolean>;
};

export function useGiteaAccessToken(
	projectId: Reactive<string>,
	repoDomain: Reactive<string | undefined> = reactive(() => undefined),
): GiteaAccess {
	const giteaUserService = inject(GITEA_USER_SERVICE);
	const { preferredGiteaAccount } = usePreferredGiteaUsername(projectId, repoDomain);
	const giteaUserResponse = $derived.by(() => {
		if (preferredGiteaAccount.current === undefined) return undefined;
		return giteaUserService.authenticatedUser(preferredGiteaAccount.current);
	});
	const accessToken = $derived(giteaUserResponse?.response?.accessToken);
	const host = $derived.by(() => {
		if (preferredGiteaAccount.current?.type === "selfHosted") {
			return preferredGiteaAccount.current.info.host;
		}
		return undefined;
	});
	return {
		host: reactive(() => host),
		accessToken: reactive(() => accessToken),
		isLoading: reactive(() => giteaUserResponse?.result.isLoading ?? false),
		error: reactive(
			() => giteaUserResponse?.result.error as { code?: Code; message: string } | undefined,
		),
		isError: reactive(() => giteaUserResponse?.result.isError ?? false),
	};
}
