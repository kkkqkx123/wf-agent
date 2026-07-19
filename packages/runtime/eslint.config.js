import rootConfig from "../../eslint.config.js";

export default [
  ...rootConfig,
  {
    files: ["**/*.ts"],
    rules: {
      // runtime specific rules can be added here
    },
  },
];