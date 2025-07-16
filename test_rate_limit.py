#!/usr/bin/env python3
"""
测试速率限制功能的脚本
"""

import requests
import time
import json
from concurrent.futures import ThreadPoolExecutor, as_completed

def test_hls_rate_limit():
    """测试HLS请求的速率限制"""
    url = "http://localhost:3001/wida/wida.m3u8"
    
    print("🧪 测试HLS请求速率限制...")
    print(f"配置: 100请求/60秒窗口，突发允许20")
    
    success_count = 0
    rate_limited_count = 0
    
    # 快速发送多个请求
    for i in range(25):  # 超过突发允许量
        try:
            response = requests.get(url, timeout=5)
            if response.status_code == 200:
                success_count += 1
                print(f"✅ 请求 {i+1}: 成功")
            elif response.status_code == 429:
                rate_limited_count += 1
                print(f"⚠️  请求 {i+1}: 速率限制 (429)")
            else:
                print(f"❌ 请求 {i+1}: 其他错误 ({response.status_code})")
        except Exception as e:
            print(f"❌ 请求 {i+1}: 异常 - {e}")
        
        time.sleep(0.1)  # 短暂延迟
    
    print(f"\n📊 结果统计:")
    print(f"   成功请求: {success_count}")
    print(f"   速率限制: {rate_limited_count}")
    print(f"   总请求数: {success_count + rate_limited_count}")

def test_concurrent_requests():
    """测试并发请求的速率限制"""
    url = "http://localhost:3001/wida/wida.m3u8"
    
    print("\n🧪 测试并发请求速率限制...")
    
    def make_request(request_id):
        try:
            response = requests.get(url, timeout=5)
            return {
                'id': request_id,
                'status': response.status_code,
                'success': response.status_code == 200,
                'rate_limited': response.status_code == 429
            }
        except Exception as e:
            return {
                'id': request_id,
                'status': 'error',
                'success': False,
                'rate_limited': False,
                'error': str(e)
            }
    
    # 并发发送请求
    with ThreadPoolExecutor(max_workers=10) as executor:
        futures = [executor.submit(make_request, i) for i in range(30)]
        results = [future.result() for future in as_completed(futures)]
    
    success_count = sum(1 for r in results if r['success'])
    rate_limited_count = sum(1 for r in results if r['rate_limited'])
    error_count = sum(1 for r in results if r['status'] == 'error')
    
    print(f"📊 并发测试结果:")
    print(f"   成功请求: {success_count}")
    print(f"   速率限制: {rate_limited_count}")
    print(f"   错误请求: {error_count}")
    print(f"   总请求数: {len(results)}")

def check_rate_limit_config():
    """检查当前的速率限制配置"""
    print("🔧 当前速率限制配置:")
    
    # 读取配置文件
    try:
        with open('conf.yaml', 'r', encoding='utf-8') as f:
            content = f.read()
            
        # 简单解析配置（仅用于显示）
        lines = content.split('\n')
        in_rate_limit = False
        
        for line in lines:
            if 'rate_limit:' in line:
                in_rate_limit = True
                print(f"   {line}")
            elif in_rate_limit and line.startswith('  '):
                print(f"   {line}")
            elif in_rate_limit and not line.startswith(' '):
                break
                
    except Exception as e:
        print(f"   ❌ 无法读取配置文件: {e}")

if __name__ == "__main__":
    print("🚀 速率限制测试工具")
    print("=" * 50)
    
    # 检查配置
    check_rate_limit_config()
    
    # 等待服务准备
    print("\n⏳ 等待服务准备...")
    time.sleep(2)
    
    # 运行测试
    test_hls_rate_limit()
    test_concurrent_requests()
    
    print("\n✅ 测试完成!")
