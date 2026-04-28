import rootConfig from "../../eslint.config.js";

export default [
  ...rootConfig,
  {
    files: ["**/*.ts"],
    rules: {
      // tool-executors specific rules can be added here
    },
  },
];
