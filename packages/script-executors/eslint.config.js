import rootConfig from "../../eslint.config.js";

export default [
  ...rootConfig,
  {
    files: ["**/*.ts"],
    rules: {
      // script-executors specific rules can be added here
    },
  },
];
