/** @type {import('jest').Config} */
module.exports = {
  testEnvironment: "node",
  roots: ["<rootDir>/src"],
  testMatch: ["**/*.test.ts"],
  moduleFileExtensions: ["ts", "js", "json"],
  // Transform TypeScript with @swc/jest (TS-version-agnostic — decoupled from the
  // TS compiler API so it keeps working across TypeScript major bumps, unlike
  // ts-jest which peer-caps at `<7`). Type-checking still happens in `npm run
  // build` (tsc); this transform only transpiles for the test run.
  transform: {
    "^.+\\.(t|j)s$": [
      "@swc/jest",
      {
        jsc: {
          parser: { syntax: "typescript" },
          target: "es2022",
        },
        module: { type: "commonjs" },
      },
    ],
  },
  // The SDK's TypeScript uses NodeNext-style `import ... from "./types.js"`
  // even though the source file is `types.ts`. Jest's resolver needs this
  // mapping to strip the `.js` suffix and find the compiled-or-source form.
  moduleNameMapper: {
    "^(\\.{1,2}/.*)\\.js$": "$1",
  },
};
