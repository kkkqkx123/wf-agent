/**
 * execute_command 工具注册
 */

import { registerTool } from '../../toolRegistry'
import ExecuteCommandComponent from '../../../components/tools/terminal/execute_command.vue'

// 注册 execute_command 工具
registerTool('execute_command', {
  name: 'execute_command',
  label: '执行命令',
  icon: 'codicon-terminal',
  
  // 描述生成器 - 显示命令
  descriptionFormatter: (args) => {
    const command = args.command as string || ''
    const cwd = args.cwd as string
    const shell = args.shell as string
    
    let desc = command
    if (cwd) {
      desc += `\n目录: ${cwd}`
    }
    if (shell && shell !== 'default') {
      desc += `\nShell: ${shell}`
    }
    return desc
  },
  
  // 使用自定义组件显示内容
  contentComponent: ExecuteCommandComponent
})