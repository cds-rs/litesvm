import { AccountRole, generateKeyPairSigner, lamports } from "@solana/kit";
import {
	cpiTree,
	formatCpiTree,
	formatCpiTreeWith,
	LiteSVM,
	TransactionMetadata,
} from "litesvm";
import assert from "node:assert/strict";
import { test } from "node:test";
import {
	generateAddress,
	getSignedTransaction,
	LAMPORTS_PER_SOL,
} from "./util";

const OUTER = "De1egAFMkMWZSN5rYXRj9CAdheBamobVNubTsi9avR44";
const INNER = "11111111111111111111111111111111";

const FIXTURE_LOGS = [
	`Program ${OUTER} invoke [1]`,
	"Program log: Instruction: Subscribe",
	`Program ${INNER} invoke [2]`,
	`Program ${INNER} success`,
	"Program data: SGVsbG8=",
	`Program ${OUTER} consumed 6540 of 200000 compute units`,
	`Program ${OUTER} success`,
];

test("formatCpiTree renders box art under the header", () => {
	const rendered = formatCpiTree("Transaction", cpiTree(FIXTURE_LOGS));
	assert.strictEqual(
		rendered,
		[
			"Transaction",
			`└── Subscribe (6,540 / 200,000 CU) ${OUTER}`,
			"    │ >> data: SGVsbG8=",
			`    └── ${INNER}`,
			"",
		].join("\n"),
	);
});

test("formatCpiTreeWith labels programs through the callback", () => {
	const aliases: Record<string, string> = {
		[OUTER]: "subscriptions",
		[INNER]: "System",
	};
	const seen: string[] = [];
	const rendered = formatCpiTreeWith(
		"Transaction",
		cpiTree(FIXTURE_LOGS),
		(programId) => {
			seen.push(programId);
			return aliases[programId] ?? programId;
		},
	);
	assert.strictEqual(
		rendered,
		[
			"Transaction",
			"└── Subscribe (6,540 / 200,000 CU) subscriptions",
			"    │ >> data: SGVsbG8=",
			"    └── System",
			"",
		].join("\n"),
	);
	// One call per distinct program id, in tree order.
	assert.deepStrictEqual(seen, [OUTER, INNER]);
});

test("formatCpiTreeWith propagates a throwing callback", () => {
	assert.throws(
		() =>
			formatCpiTreeWith("Transaction", cpiTree(FIXTURE_LOGS), () => {
				throw new Error("boom");
			}),
		/boom/,
	);
});

test("formatCpiTree rejects a frame with a non-base58 program id", () => {
	assert.throws(
		() =>
			formatCpiTree("Transaction", [
				{
					programId: "not-base58!",
					outcome: { type: "success" },
					logs: [],
					children: [],
				},
			]),
		/invalid program id 'not-base58!'/,
	);
});

test("TransactionMetadata.prettyCpiTree renders a live transaction", async () => {
	const [payer, programAddress, loggedAddress] = await Promise.all([
		generateKeyPairSigner(),
		generateAddress(),
		generateAddress(),
	]);

	const svm = new LiteSVM();
	svm.airdrop(payer.address, lamports(LAMPORTS_PER_SOL));
	svm.addProgramFromFile(
		programAddress,
		"program_bytes/spl_example_logging.so",
	);

	const transaction = await getSignedTransaction(svm, payer, [
		{
			accounts: [{ address: loggedAddress, role: AccountRole.READONLY }],
			programAddress,
		},
	]);
	const result = svm.sendTransaction(transaction);
	if (!(result instanceof TransactionMetadata)) {
		throw new Error("Unexpected tx failure");
	}

	const rendered = result.prettyCpiTree();
	assert.ok(rendered.includes(programAddress));
	assert.ok(rendered.includes("└──"));
	// The synthetic header and the frame agree on consumed CU.
	assert.strictEqual(
		rendered,
		formatCpiTree(rendered.split("\n")[0], result.cpiTree()),
	);
});
