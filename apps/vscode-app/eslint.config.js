import rootConfig from "../../eslint.config.js";

export default [
  ...rootConfig,
  {
    files: ["**/*.ts", "**/*.tsx"],
    rules: {
      // vscode-app specific rules can be added here
    },
  },
];
