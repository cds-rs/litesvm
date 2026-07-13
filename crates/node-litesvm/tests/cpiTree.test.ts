import { AccountRole, generateKeyPairSigner, lamports } from "@solana/kit";
import { cpiTree, LiteSVM, TransactionMetadata } from "litesvm";
import assert from "node:assert/strict";
import { test } from "node:test";
import {
	generateAddress,
	getSignedTransaction,
	LAMPORTS_PER_SOL,
} from "./util";

const TOKEN_PROGRAM = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
const SYSTEM_PROGRAM = "11111111111111111111111111111111";

test("cpiTree parses logs into plain JSON data", () => {
	const tree = cpiTree([
		`Program ${TOKEN_PROGRAM} invoke [1]`,
		"Program log: Instruction: Transfer",
		`Program ${SYSTEM_PROGRAM} invoke [2]`,
		`Program ${SYSTEM_PROGRAM} success`,
		`Program ${TOKEN_PROGRAM} consumed 4645 of 200000 compute units`,
		`Program ${TOKEN_PROGRAM} success`,
	]);

	assert.strictEqual(tree.length, 1);
	const root = tree[0];
	assert.strictEqual(root.programId, TOKEN_PROGRAM);
	assert.strictEqual(root.outcome.type, "success");
	assert.strictEqual(root.computeUnits?.consumed, 4645);
	assert.strictEqual(root.computeUnits?.availableAtStart, 200000);
	assert.strictEqual(root.children.length, 1);
	assert.strictEqual(root.children[0].programId, SYSTEM_PROGRAM);

	// The tree is an interchange format for downstream renderers: it must
	// survive JSON.stringify without loss (no bigints, no byte arrays).
	assert.deepStrictEqual(JSON.parse(JSON.stringify(tree)), tree);
});

test("cpiTree rounds availableAtStart above 2^53", () => {
	// A budget echo above Number.MAX_SAFE_INTEGER (reachable via
	// ComputeBudget.computeUnitLimit) rounds instead of throwing;
	// u64::MAX rounds to exactly 2^64.
	const tree = cpiTree([
		`Program ${TOKEN_PROGRAM} invoke [1]`,
		`Program ${TOKEN_PROGRAM} consumed 100 of 18446744073709551615 compute units`,
		`Program ${TOKEN_PROGRAM} success`,
	]);

	assert.strictEqual(tree[0].computeUnits?.consumed, 100);
	assert.strictEqual(tree[0].computeUnits?.availableAtStart, 2 ** 64);
});

test("TransactionMetadata.cpiTree returns the same shape", async () => {
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

	const tree = result.cpiTree();
	assert.strictEqual(tree.length, 1);
	const root = tree[0];
	assert.strictEqual(root.programId, programAddress);
	assert.strictEqual(root.outcome.type, "success");
	assert.strictEqual(typeof root.computeUnits?.consumed, "number");
	assert.ok(root.computeUnits.consumed > 0);
	assert.deepStrictEqual(tree, cpiTree(result.logs()));
	assert.deepStrictEqual(JSON.parse(JSON.stringify(tree)), tree);
});
