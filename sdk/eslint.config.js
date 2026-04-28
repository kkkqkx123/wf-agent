import rootConfig from "../eslint.config.js";

export default [
  ...rootConfig,
  {
    files: ["**/*.ts"],
    rules: {
      // SDK specific rules can be added here
    },
  },
];
