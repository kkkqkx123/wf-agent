import rootConfig from "../../eslint.config.js";

export default [
  ...rootConfig,
  {
    files: ["**/*.ts"],
    rules: {
      // prompt-templates specific rules can be added here
    },
  },
];
