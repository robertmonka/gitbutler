import {
	giteaAccountIdentifierToString,
	stringToGiteaAccountIdentifier,
	UNIT_SEP,
} from "$lib/forge/gitea/giteaUserService.svelte";
import { describe, expect, test } from "vitest";
import type { GiteaAccountIdentifier } from "@gitbutler/but-sdk";

describe("GiteaUserService", () => {
	test("parses self-hosted account identifiers", () => {
		expect(
			stringToGiteaAccountIdentifier(
				`selfHosted${UNIT_SEP}https://gitea.example.com${UNIT_SEP}alice`,
			),
		).toStrictEqual({
			type: "selfHosted",
			info: {
				host: "https://gitea.example.com",
				username: "alice",
			},
		});
	});

	test("rejects malformed account identifiers", () => {
		expect(stringToGiteaAccountIdentifier("selfHosted")).toBeNull();
		expect(stringToGiteaAccountIdentifier(`selfHosted${UNIT_SEP}only-host`)).toBeNull();
		expect(stringToGiteaAccountIdentifier(`patUsername${UNIT_SEP}alice`)).toBeNull();
	});

	test("round-trips account identifiers", () => {
		const account: GiteaAccountIdentifier = {
			type: "selfHosted",
			info: {
				host: "https://gitea.example.com",
				username: "alice",
			},
		};

		const serialized = giteaAccountIdentifierToString(account);

		expect(stringToGiteaAccountIdentifier(serialized)).toStrictEqual(account);
	});
});
