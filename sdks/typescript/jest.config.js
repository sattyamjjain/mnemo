/** @type {import('ts-jest').JestConfigWithTsJest} */
module.exports = {
  preset: "ts-jest",
  testEnvironment: "node",
  roots: ["<rootDir>/src"],
  testMatch: ["**/*.test.ts"],
  moduleFileExtensions: ["ts", "js", "json"],
  // The SDK's TypeScript uses NodeNext-style `import ... from "./types.js"`
  // even though the source file is `types.ts`. Jest's resolver needs this
  // mapping to strip the `.js` suffix and find the compiled-or-source form.
  moduleNameMapper: {
    "^(\\.{1,2}/.*)\\.js$": "$1",
  },
};
