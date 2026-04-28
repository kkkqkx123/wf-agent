/**
 * find_files 工具注册
 */

import { registerTool } from '../../toolRegistry'
import FindFilesComponent from '../../../components/tools/search/find_files.vue'

// 注册 find_files 工具
registerTool('find_files', {
  name: 'find_files',
  label: '查找文件',
  icon: 'codicon-search',
  
  // 描述生成器 - 显示查找模式（每行一个）
  descriptionFormatter: (args) => {
    if (args.patterns && Array.isArray(args.patterns)) {
      return (args.patterns as string[]).join('\n')
    }
    if (args.pattern) {
      return args.pattern as string
    }
    return '查找文件'
  },
  
  // 使用自定义组件显示内容
  contentComponent: FindFilesComponent
})