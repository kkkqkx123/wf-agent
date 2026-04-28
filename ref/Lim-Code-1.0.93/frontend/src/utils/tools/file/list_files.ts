/**
 * list_files 工具注册
 */

import { registerTool } from '../../toolRegistry'
import ListFilesComponent from '../../../components/tools/file/list_files.vue'

// 注册 list_files 工具
registerTool('list_files', {
  name: 'list_files',
  label: '列出文件',
  icon: 'codicon-folder',
  
  // 描述生成器 - 显示目录路径或数量
  descriptionFormatter: (args) => {
    const recursive = args.recursive as boolean
    const suffix = recursive ? ' (递归)' : ''
    
    if (args.paths && Array.isArray(args.paths)) {
      const count = (args.paths as string[]).length
      if (count === 1) {
        return `${args.paths[0]}${suffix}`
      }
      return `${count} 个目录${suffix}`
    }
    if (args.path) {
      return `${args.path}${suffix}`
    }
    return `.${suffix}`
  },
  
  // 使用自定义组件显示内容
  contentComponent: ListFilesComponent
})