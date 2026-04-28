# Mini-Agent Document Skills 分析

## 概述

Mini-Agent 的 document-skills 模块提供了一套完整的文档处理能力，支持 Word(docx)、PDF、PowerPoint(pptx) 和 Excel(xlsx) 四种主流文档格式。该模块采用渐进式披露设计模式，让 LLM 能够按需学习和使用各种文档处理技能。

## 技能提供方式

### 1. 技能定义结构

每个文档技能都遵循统一的目录结构：
```
document-skills/
├── docx/
│   ├── SKILL.md          # 技能定义文档
│   └── scripts/          # 实现脚本
│       ├── document.py   # 核心实现
│       └── utilities.py  # 工具函数
├── pdf/
│   ├── SKILL.md
│   └── scripts/
│       ├── fill_pdf_form_with_annotations.py
│       └── ...
├── pptx/
│   ├── SKILL.md
│   └── scripts/
│       ├── html2pptx.js
│       └── ...
└── xlsx/
    ├── SKILL.md
    └── scripts/
        └── recalc.py
```

### 2. 技能文档格式

每个 `SKILL.md` 文件包含：
- **技能名称和描述**：明确技能的功能范围
- **功能特性**：详细列出支持的操作类型
- **代码示例**：提供具体的使用方法和参数说明
- **工作流程**：描述完整的处理步骤
- **最佳实践**：包含设计原则和质量要求

例如，docx 技能的 SKILL.md 定义了：
- 文档创建、编辑和分析功能
- XML 访问和文本提取方法
- 红线修订工作流
- 文档转图片操作

### 3. 实现脚本架构

每个技能目录下的 `scripts/` 包含具体的实现代码：

**docx/document.py**：
- `DocxXMLEditor` 类：继承自 `XMLEditor`，自动处理 RSID、作者和日期属性
- `Document` 类：提供文档操作接口，支持节点查找、注释添加、修订跟踪
- 命名空间管理：处理 Word 文档的 XML 命名空间

**pdf/fill_pdf_form_with_annotations.py**：
- `transform_coordinates()`：图像坐标到 PDF 坐标的转换
- `fill_pdf_form()`：使用 FreeText 注释填充 PDF 表单
- 支持字体、大小、颜色设置和页面尺寸处理

**pptx/html2pptx.js**：
- HTML 到 pptxgenjs 的转换器
- 支持文本、图片、形状和项目符号列表
- 提供尺寸验证和内容溢出检查

**xlsx/recalc.py**：
- LibreOffice 宏设置和公式重计算
- Excel 错误检测（#VALUE!、#DIV/0! 等）
- 全工作表扫描和错误位置报告

## LLM 学习机制

### 1. 渐进式披露（Level 1-3）

**Level 1 - 元数据层**：
- 系统只向 LLM 暴露技能名称和简短描述
- LLM 通过 `get_skill` 工具按需获取详细信息
- 避免一次性加载所有技能信息导致上下文溢出

**Level 2 - 详细内容层**：
- 当 LLM 调用 `get_skill` 时，返回完整的 SKILL.md 内容
- 包含详细的功能描述、代码示例和使用方法
- LLM 可以根据具体需求学习相应的技能

**Level 3+ - 资源层**：
- 提供实现脚本的源代码和依赖信息
- 支持深度学习和自定义修改
- 包含最佳实践和质量标准

### 2. 技能加载器（skill_loader.py）

```python
@dataclass
class Skill:
    name: str
    description: str
    content: str
    yaml_header: Dict[str, Any]
    file_path: str

class SkillLoader:
    def load_skill(self, file_path: str) -> Skill:
        # 解析 SKILL.md 文件的 YAML 头部和 Markdown 内容
        # 创建 Skill 对象并处理路径转换
    
    def discover_skills(self, directory: str) -> List[Skill]:
        # 递归发现指定目录下的所有 SKILL.md 文件
    
    def get_skills_metadata_prompt(self) -> str:
        # 生成只包含名称和描述的元数据提示
```

### 3. 工具集成（skill_tool.py）

```python
class GetSkillTool:
    def __init__(self, skill_loader: SkillLoader):
        self.skill_loader = skill_loader
    
    def execute(self, skill_name: str) -> str:
        # 获取指定技能的详细信息
        # 返回 SKILL.md 的完整内容

def create_skill_tools(skills_directory: str) -> List:
    # 创建技能加载器
    # 只提供 get_skill 工具，实现按需加载
```

### 4. Agent 集成（agent.py）

```python
class Agent:
    def __init__(self, llm_client, system_prompt, tools=None):
        self.tools = {tool.name: tool for tool in tools} if tools else {}
        # 工具以字典形式存储，支持动态调用
```

### 5. 系统提示设计（system_prompt.md）

系统提示包含：
- **核心能力说明**：基础工具和专业技能的区别
- **渐进式披露机制**：三级信息披露的详细说明
- **使用步骤**：如何发现、获取和使用技能
- **环境管理**：Python 环境要求和 uv 工具使用
- **执行指南**：文件操作和 bash 命令规范

关键占位符：
```markdown
{SKILLS_METADATA}  # 替换为技能元数据列表
```

## 工作流程示例

### 1. 技能发现阶段
1. Agent 初始化时加载所有技能的元数据
2. 系统提示中包含可用技能的名称和描述
3. LLM 了解有哪些文档处理技能可用

### 2. 技能学习阶段
1. 当 LLM 需要处理特定文档类型时
2. 调用 `get_skill("docx")` 获取详细信息
3. 学习该技能的功能、使用方法和代码示例

### 3. 技能应用阶段
1. LLM 根据学到的知识生成处理代码
2. 执行相应的脚本文件
3. 处理文档并返回结果

## 设计优势

### 1. 可扩展性
- 统一的技能定义格式，易于添加新技能
- 模块化设计，技能之间相互独立
- 支持自定义实现脚本

### 2. 高效性
- 按需加载，避免信息过载
- 渐进式学习，降低学习成本
- 缓存机制，避免重复加载

### 3. 可维护性
- 清晰的目录结构
- 标准化的文档格式
- 完整的代码示例

### 4. 质量保证
- 每个技能都有详细的使用说明
- 包含最佳实践和设计原则
- 提供错误处理和验证机制

## 总结

Mini-Agent 的 document-skills 模块通过统一的技能定义格式、渐进式的学习机制和模块化的实现架构，为 LLM 提供了强大的文档处理能力。这种设计既保证了系统的可扩展性和维护性，又通过按需加载机制优化了资源使用，是一种高效且实用的技能管理系统。