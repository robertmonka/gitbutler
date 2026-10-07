import { invalidatesType, providesItem, providesList, ReduxTag } from "$lib/state/tags";
import { InjectionToken } from "@gitbutler/core/context";
import type { BackendApi } from "$lib/state/backendApi";
import type {
	GiteaAccountIdentifier,
	GiteaAuthenticatedUserSensitive,
	GiteaAuthStatusResponseSensitive,
} from "@gitbutler/but-sdk";

export const GITEA_USER_SERVICE = new InjectionToken<GiteaUserService>("GiteaUserService");

function normalizeHostForComparison(value: string): string {
	const withoutScheme = value.includes("://") ? (value.split("://")[1] ?? value) : value;
	const withoutUserInfo = withoutScheme.includes("@")
		? (withoutScheme.split("@").at(-1) ?? withoutScheme)
		: withoutScheme;
	const host = withoutUserInfo.split(/[/?#]/)[0]?.split(":")[0] ?? "";
	return host.trim().replace(/\.$/, "").toLowerCase();
}

/** Whether the API or view URL contains the `gitea` hostname marker. */
export function giteaHostsContainKeyword(apiHost: string, viewHost?: string): boolean {
	return apiHost.includes("gitea") || (viewHost?.includes("gitea") ?? false);
}

function parentDomainSuffix(host: string): string | undefined {
	const labels = host.split(".").filter(Boolean);
	if (labels.length < 2) {
		return undefined;
	}
	return labels.slice(-2).join(".");
}

function hostsShareParentDomain(repositoryHost: string, accountHost: string): boolean {
	if (repositoryHost === accountHost) {
		return true;
	}
	const repositoryParent = parentDomainSuffix(repositoryHost);
	const accountParent = parentDomainSuffix(accountHost);
	return (
		repositoryParent !== undefined &&
		repositoryParent === accountParent &&
		repositoryParent.includes(".")
	);
}

/** Whether a stored Gitea account belongs to the repository remote host. */
export function giteaAccountMatchesRepoDomain(
	account: GiteaAccountIdentifier,
	repoDomain: string | undefined,
): boolean {
	if (!repoDomain) {
		return false;
	}
	const repositoryHost = normalizeHostForComparison(repoDomain);
	if (!repositoryHost) {
		return false;
	}
	if (account.type === "selfHosted") {
		const hosts = [account.info.host, account.info.viewHost].filter(
			(host): host is string => Boolean(host),
		);
		if (
			hosts.some((host) => {
				const accountHost = normalizeHostForComparison(host);
				return (
					accountHost === repositoryHost || accountHost.endsWith(`.${repositoryHost}`)
				);
			})
		) {
			return true;
		}

		if (!giteaHostsContainKeyword(account.info.host, account.info.viewHost)) {
			return false;
		}

		return hosts.some((host) =>
			hostsShareParentDomain(repositoryHost, normalizeHostForComparison(host)),
		);
	}
	return repositoryHost.includes("gitea");
}

export function isSameGiteaAccountIdentifier(
	a: GiteaAccountIdentifier,
	b: GiteaAccountIdentifier,
): boolean {
	if (a.type !== b.type) {
		return false;
	}
	switch (a.type) {
		case "selfHosted":
			return (
				a.info.host === (b as typeof a).info.host &&
				a.info.viewHost === (b as typeof a).info.viewHost &&
				a.info.username === (b as typeof a).info.username
			);
	}
}

export type GiteaAccountIdentifierType = GiteaAccountIdentifier["type"];

type ExhaustiveGiteaMap = Record<GiteaAccountIdentifierType, true>;

const exhaustiveGiteaMap: ExhaustiveGiteaMap = {
	selfHosted: true,
};

function isGiteaAccountIdentifierType(text: unknown): text is GiteaAccountIdentifierType {
	if (typeof text !== "string") {
		return false;
	}
	return exhaustiveGiteaMap[text as GiteaAccountIdentifierType] ?? false;
}

// ASCII Unit Separator, used to separate data units within a record or field.
export const UNIT_SEP = "\u001F";

export function giteaAccountIdentifierToString(account: GiteaAccountIdentifier): string {
	switch (account.type) {
		case "selfHosted": {
			const viewSuffix = account.info.viewHost
				? `${UNIT_SEP}${account.info.viewHost}`
				: "";
			return `${account.type}${UNIT_SEP}${account.info.host}${UNIT_SEP}${account.info.username}${viewSuffix}`;
		}
	}
}

export function stringToGiteaAccountIdentifier(str: string): GiteaAccountIdentifier | null {
	const parts = str.split(UNIT_SEP);
	if (parts.length < 2) {
		return null;
	}
	const [type, ...infoParts] = parts;

	if (!isGiteaAccountIdentifierType(type)) {
		return null;
	}

	switch (type) {
		case "selfHosted":
			if (infoParts.length < 2) return null;

			return {
				type: "selfHosted",
				info: {
					host: infoParts[0]!,
					username: infoParts[1]!,
					...(infoParts[2] ? { viewHost: infoParts[2] } : {}),
				},
			};
	}
}

export class GiteaUserService {
	private backendApi: ReturnType<typeof injectBackendEndpoints>;

	constructor(backendApi: BackendApi) {
		this.backendApi = injectBackendEndpoints(backendApi);
	}

	get storeGiteaSelfHostedPat() {
		return this.backendApi.endpoints.storeGiteaSelfHostedPat.useMutation();
	}

	get forgetGiteaAccount() {
		return this.backendApi.endpoints.forgetGiteaAccount.useMutation();
	}

	authenticatedUser(account: GiteaAccountIdentifier) {
		return this.backendApi.endpoints.getGiteaUser.useQuery({ account });
	}

	accounts() {
		return this.backendApi.endpoints.listKnownGiteaAccounts.useQuery();
	}

	deleteAllGiteaAccounts() {
		return this.backendApi.endpoints.clearAllGiteaAccounts.useMutation();
	}
}

function injectBackendEndpoints(api: BackendApi) {
	return api.injectEndpoints({
		endpoints: (build) => ({
			forgetGiteaAccount: build.mutation<void, GiteaAccountIdentifier>({
				extraOptions: {
					command: "forget_gitea_account",
					actionName: "Forget Gitea Account",
				},
				query: (account) => ({ account }),
				invalidatesTags: [providesList(ReduxTag.GiteaUserList)],
			}),
			getGiteaUser: build.query<
				GiteaAuthenticatedUserSensitive | null,
				{ account: GiteaAccountIdentifier }
			>({
				extraOptions: {
					command: "get_gitea_user",
				},
				query: (args) => args,
				providesTags: (_result, _error, username) => [
					...providesItem(ReduxTag.ForgeUser, `gitea:${username}`),
				],
			}),
			listKnownGiteaAccounts: build.query<GiteaAccountIdentifier[], void>({
				extraOptions: {
					command: "list_known_gitea_accounts",
				},
				query: () => ({}),
				providesTags: [providesList(ReduxTag.GiteaUserList)],
			}),
			clearAllGiteaAccounts: build.mutation<void, void>({
				extraOptions: {
					command: "clear_all_gitea_tokens",
					actionName: "Clear All Gitea Accounts",
				},
				query: () => ({}),
				invalidatesTags: [providesList(ReduxTag.GiteaUserList)],
			}),
			storeGiteaSelfHostedPat: build.mutation<
				GiteaAuthStatusResponseSensitive,
				{ host: string; viewHost?: string; accessToken: string }
			>({
				extraOptions: {
					command: "store_gitea_selfhosted_pat",
					actionName: "Store Gitea PAT",
				},
				query: (args) => args,
				invalidatesTags: [
					providesList(ReduxTag.GiteaUserList),
					invalidatesType(ReduxTag.ForgeProvider),
				],
			}),
		}),
	});
}
