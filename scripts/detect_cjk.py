#!/usr/bin/env python3
"""
检测文件中的 CJK (中日韩) 字符并生成报告。

用法:
    python detect_cjk.py <file_path|directory_path> [--output <report_path>] [--recursive]
"""

import sys
import argparse
from pathlib import Path
from typing import List, Tuple, Dict
from datetime import datetime


def is_cjk_char(char: str) -> bool:
    """
    判断字符是否为 CJK 字符。
    
    CJK Unicode 范围:
    - CJK Unified Ideographs: U+4E00 - U+9FFF
    - CJK Unified Ideographs Extension A: U+3400 - U+4DFF
    - CJK Unified Ideographs Extension B: U+20000 - U+2A6DF
    - CJK Unified Ideographs Extension C: U+2A700 - U+2B73F
    - CJK Unified Ideographs Extension D: U+2B740 - U+2B81F
    - CJK Unified Ideographs Extension E: U+2B820 - U+2CEAF
    - CJK Unified Ideographs Extension F: U+2CEB0 - U+2EBEF
    - CJK Compatibility Ideographs: U+F900 - U+FAFF
    - CJK Compatibility Ideographs Supplement: U+2F800 - U+2FA1F
    - Hiragana (日文平假名): U+3040 - U+309F
    - Katakana (日文片假名): U+30A0 - U+30FF
    - Hangul Syllables (韩文): U+AC00 - U+D7AF
    - Hangul Jamo (韩文字母): U+1100 - U+11FF
    - CJK Radicals Supplement: U+2E80 - U+2EFF
    - Kangxi Radicals: U+2F00 - U+2FDF
    - CJK Strokes: U+31C0 - U+31EF
    """
    code_point = ord(char)
    
    cjk_ranges = [
        (0x3400, 0x4DFF),    # CJK Extension A
        (0x4E00, 0x9FFF),    # CJK Unified Ideographs
        (0xF900, 0xFAFF),    # CJK Compatibility Ideographs
        (0x3040, 0x309F),    # Hiragana
        (0x30A0, 0x30FF),    # Katakana
        (0xAC00, 0xD7AF),    # Hangul Syllables
        (0x1100, 0x11FF),    # Hangul Jamo
        (0x2E80, 0x2EFF),    # CJK Radicals Supplement
        (0x2F00, 0x2FDF),    # Kangxi Radicals
        (0x31C0, 0x31EF),    # CJK Strokes
    ]
    
    for start, end in cjk_ranges:
        if start <= code_point <= end:
            return True
    
    # 检查扩展区 B-F (代理对)
    if 0x20000 <= code_point <= 0x2EBEF:
        return True
    
    # 检查兼容表意文字补充
    if 0x2F800 <= code_point <= 0x2FA1F:
        return True
    
    return False


def is_code_file(file_path: Path, extensions: List[str] = None) -> bool:
    """
    判断文件是否为代码文件。
    
    Args:
        file_path: 文件路径
        extensions: 允许的文件扩展名列表，None 表示使用默认列表
    
    Returns:
        bool: 是否为代码文件
    """
    default_code_extensions = {
        '.rs', '.py', '.js', '.ts', '.java', '.cpp', '.c', '.h', '.hpp',
        '.go', '.rb', '.php', '.swift', '.kt', '.scala', '.sh', '.bash',
        '.sql', '.css', '.html', '.xml', '.json', '.yaml', '.yml', '.toml',
        '.md', '.txt', '.graphql', '.gql'
    }
    
    use_extensions = extensions if extensions is not None else default_code_extensions
    
    return file_path.suffix.lower() in use_extensions


def find_cjk_in_file(file_path: Path, extensions: List[str] = None) -> List[Tuple[int, str, List[str]]]:
    """
    在文件中查找所有包含 CJK 字符的行。
    
    Args:
        file_path: 文件路径
        extensions: 要扫描的文件扩展名列表，None 表示扫描所有代码文件
    
    Returns:
        [(行号，行内容，[CJK 字符列表]), ...]
    """
    if not is_code_file(file_path, extensions):
        return []
    
    results = []
    
    try:
        # 尝试多种编码读取文件
        encodings = ['utf-8', 'utf-8-sig', 'gbk', 'gb2312', 'big5', 'shift_jis']
        content = None
        
        for encoding in encodings:
            try:
                with open(file_path, 'r', encoding=encoding) as f:
                    content = f.readlines()
                break
            except (UnicodeDecodeError, UnicodeError):
                continue
        
        if content is None:
            print(f"错误：无法使用支持的编码读取文件 {file_path}")
            return results
        
        for line_num, line in enumerate(content, start=1):
            cjk_chars = []
            for char in line:
                if is_cjk_char(char) and char not in cjk_chars:
                    cjk_chars.append(char)
            
            if cjk_chars:
                results.append((line_num, line.rstrip('\n\r'), cjk_chars))
    
    except FileNotFoundError:
        print(f"错误：文件不存在：{file_path}")
    except PermissionError:
        print(f"错误：无权限读取文件：{file_path}")
    except Exception as e:
        print(f"错误：读取文件时发生异常：{e}")
    
    return results


def find_cjk_in_directory(directory_path: Path, recursive: bool = False, 
                          extensions: List[str] = None) -> Dict[Path, List[Tuple[int, str, List[str]]]]:
    """
    在目录中查找所有包含 CJK 字符的文件。
    
    Args:
        directory_path: 目录路径
        recursive: 是否递归扫描
        extensions: 要扫描的文件扩展名列表，None 表示扫描所有代码文件
    
    Returns:
        {文件路径: [(行号，行内容，[CJK 字符列表]), ...], ...}
    """
    results = {}
    
    try:
        if recursive:
            file_pattern = "**/*"
        else:
            file_pattern = "*"
        
        for file_path in directory_path.glob(file_pattern):
            if file_path.is_file():
                file_results = find_cjk_in_file(file_path, extensions)
                if file_results:
                    results[file_path] = file_results
    
    except PermissionError:
        print(f"错误：无权限访问目录：{directory_path}")
    except Exception as e:
        print(f"错误：扫描目录时发生异常：{e}")
    
    return results


def generate_report(file_path: Path, results: List[Tuple[int, str, List[str]]], 
                    output_path: Path = None, is_directory: bool = False,
                    extensions: List[str] = None) -> str:
    """
    生成 CJK 字符检测报告。
    
    Args:
        file_path: 检测的文件或目录路径
        results: 检测结果
        output_path: 输出路径
        is_directory: 是否为目录
        extensions: 扫描的文件扩展名
    
    Returns:
        报告字符串
    """
    timestamp = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
    
    if is_directory:
        total_files = len(results)
        total_lines = sum(len(file_results) for file_results in results.values())
        
        ext_filter = ', '.join(sorted(extensions)) if extensions else 'all code files'
        
        report_lines = [
            "=" * 80,
            "CJK 字符检测报告",
            "=" * 80,
            "",
            f"检测目录：{file_path.absolute()}",
            f"检测时间：{timestamp}",
            f"扫描的文件类型：{ext_filter}",
            f"包含 CJK 字符的文件数：{total_files}",
            f"包含 CJK 字符的总行数：{total_lines}",
            "",
            "-" * 80,
        ]
        
        if results:
            report_lines.append("详细结果:")
            report_lines.append("-" * 80)
            report_lines.append("")
            
            for file_path, file_results in sorted(results.items()):
                relative_path = file_path.relative_to(file_path.parent if is_directory else file_path.parent.parent)
                report_lines.append(f"文件：{relative_path}")
                report_lines.append("-" * 80)
                
                for line_num, line_content, cjk_chars in file_results:
                    report_lines.append(f"  行 {line_num}:")
                    report_lines.append(f"    CJK 字符：{', '.join(f'U+{ord(c):04X}({c})' for c in cjk_chars)}")
                    report_lines.append(f"    内容：{line_content[:100]}{'...' if len(line_content) > 100 else ''}")
                    report_lines.append("")
                
                report_lines.append("")
        else:
            report_lines.append("未检测到 CJK 字符。")
    else:
        report_lines = [
            "=" * 80,
            "CJK 字符检测报告",
            "=" * 80,
            "",
            f"检测文件：{file_path.absolute()}",
            f"检测时间：{timestamp}",
            f"总行数：{sum(1 for _ in open(file_path, 'r', encoding='utf-8', errors='ignore')) if file_path.exists() else 'N/A'}",
            f"包含 CJK 字符的行数：{len(results)}",
            "",
            "-" * 80,
        ]
        
        if results:
            report_lines.append("详细结果:")
            report_lines.append("-" * 80)
            report_lines.append("")
            
            for line_num, line_content, cjk_chars in results:
                report_lines.append(f"行 {line_num}:")
                report_lines.append(f"  CJK 字符：{', '.join(f'U+{ord(c):04X}({c})' for c in cjk_chars)}")
                report_lines.append(f"  内容：{line_content[:100]}{'...' if len(line_content) > 100 else ''}")
                report_lines.append("")
        else:
            report_lines.append("未检测到 CJK 字符。")
    
    report_lines.append("-" * 80)
    report_lines.append("报告结束")
    report_lines.append("=" * 80)
    
    report = "\n".join(report_lines)
    
    if output_path:
        output_path.parent.mkdir(parents=True, exist_ok=True)
        with open(output_path, 'w', encoding='utf-8') as f:
            f.write(report)
        print(f"报告已保存至：{output_path}")
    
    return report


def main():
    parser = argparse.ArgumentParser(
        description="检测文件中的 CJK (中日韩) 字符并生成报告",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
示例:
    python detect_cjk.py src/main.rs
    python detect_cjk.py src/lib.rs --output reports/cjk_report.md
    python detect_cjk.py src/api --recursive --output reports/cjk_report.md
        """
    )
    
    parser.add_argument(
        "path",
        type=Path,
        help="要检测的文件路径或目录路径"
    )
    
    parser.add_argument(
        "--output", "-o",
        type=Path,
        default=None,
        help="报告输出路径 (可选，默认输出到控制台)"
    )
    
    parser.add_argument(
        "--recursive", "-r",
        action="store_true",
        help="递归扫描目录（仅在路径为目录时有效）"
    )
    
    parser.add_argument(
        "--extensions", "-e",
        type=str,
        default=None,
        help="要扫描的文件扩展名列表，逗号分隔（例如：'.rs,.py,.js'），默认扫描所有代码文件"
    )
    
    args = parser.parse_args()
    
    if not args.path.exists():
        print(f"错误：路径不存在：{args.path}")
        sys.exit(1)
    
    is_directory = args.path.is_dir()
    
    extensions = None
    if args.extensions:
        extensions = [ext.strip() for ext in args.extensions.split(',')]
    
    if is_directory:
        # 扫描目录
        results = find_cjk_in_directory(args.path, args.recursive, extensions)
        
        # 生成报告
        if args.output:
            generate_report(args.path, results, args.output, is_directory=True, extensions=extensions)
        else:
            report = generate_report(args.path, results, is_directory=True, extensions=extensions)
            print(report)
        
        # 返回状态码
        sys.exit(0 if not results else 1)
    else:
        # 扫描单个文件
        results = find_cjk_in_file(args.path, extensions)
        
        # 生成报告
        if args.output:
            generate_report(args.path, results, args.output, is_directory=False, extensions=extensions)
        else:
            report = generate_report(args.path, results, extensions=extensions)
            print(report)
        
        # 返回状态码
        sys.exit(0 if not results else 1)


if __name__ == "__main__":
    main()
