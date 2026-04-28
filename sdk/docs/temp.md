此外，工具的基础设施见packages\tool-executors包，提示词模板见packages\prompt-templates包。需要正确理解现在项目中工具相关功能的架构。此外，工具的定义需要明确划分为3层：schema(作为代码使用的类型和提示词的一部分/给llm供应商api附加的工具schema，同时作为运行时验证的依据)、descriptions(作为提示词的一部分，指导llm的调用)、工具的代码逻辑实现。
此外，目前的工具类型是stateful、stateless、rest、mcp四种，如果需要细分也应该在这基础下组织目录结构。需要区分基于功能的分类和基于工具执行类型的划分
目前的验证逻辑见sdk\core\validation\tool-static-validator.ts、sdk\core\validation\tool-runtime-validator.ts
