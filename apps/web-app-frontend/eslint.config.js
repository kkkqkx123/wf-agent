import rootConfig from "../../eslint.config.js";

export default [
  ...rootConfig,
  {
    files: ["**/*.ts", "**/*.tsx"],
    rules: {
      // web-app-frontend specific rules can be added here
    },
  },
];
