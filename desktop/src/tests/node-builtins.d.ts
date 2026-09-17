/**
 * Minimal ambient declarations for the Node built-ins used by the Task 16
 * tests, which run on the Node built-in test runner. This keeps the tests
 * type-checkable under svelte-check without adding any dependency
 * (no test runner, no @types/node).
 */
declare module "node:test" {
  export function describe(name: string, fn: () => void | Promise<unknown>): void;
  export function it(name: string, fn: () => void | Promise<unknown>): void;
}

declare module "node:assert/strict" {
  interface StrictAssert {
    (value: unknown, message?: string): void;
    ok(value: unknown, message?: string): void;
    equal(actual: unknown, expected: unknown, message?: string): void;
    deepEqual(actual: unknown, expected: unknown, message?: string): void;
    match(value: string, pattern: RegExp, message?: string): void;
    doesNotMatch(value: string, pattern: RegExp, message?: string): void;
  }
  const assert: StrictAssert;
  export default assert;
}

declare module "node:fs" {
  export function readFileSync(path: string, encoding: "utf8"): string;
  export function existsSync(path: string): boolean;
}

declare module "node:path" {
  export function join(...parts: string[]): string;
  export function dirname(path: string): string;
}

declare module "node:url" {
  export function fileURLToPath(url: string | URL): string;
}
