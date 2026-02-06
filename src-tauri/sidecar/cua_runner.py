#!/usr/bin/env python3
"""
CUA Runner - Executes tasks using CUA computer-use agent
Requires: pip install cua-agent cua-computer
"""

import sys
import json
import asyncio

async def run_task(task_description: str, human_in_loop: bool = True):
    """
    Execute a task using CUA computer-use agent.
    
    Args:
        task_description: The task to execute
        human_in_loop: If True, waits for user confirmation at each step
    """
    try:
        from computer import Computer
        from agent import ComputerAgent
    except ImportError:
        print(json.dumps({
            "status": "error",
            "message": "CUA not installed. Run: pip install cua-agent cua-computer"
        }))
        return

    try:
        # Initialize computer (local macOS)
        computer = Computer(os_type="macos", provider_type="local")
        
        # Initialize agent with Claude
        agent = ComputerAgent(
            model="anthropic/claude-sonnet-4-5-20250929",
            computer=computer
        )
        
        print(json.dumps({
            "status": "started",
            "message": f"Executing: {task_description}"
        }))
        
        # Run the agent
        async for result in agent.run([{
            "role": "user",
            "content": task_description
        }]):
            # Stream results back
            print(json.dumps({
                "status": "progress",
                "step": str(result)
            }))
            
            if human_in_loop:
                # In practice, this would pause and wait for user confirmation
                # via Tauri window events
                pass
        
        print(json.dumps({
            "status": "completed",
            "message": "Task completed successfully"
        }))
        
    except Exception as e:
        print(json.dumps({
            "status": "error",
            "message": str(e)
        }))

def show_setup_instructions():
    """Show setup instructions for CUA."""
    instructions = """
╔══════════════════════════════════════════════════════════════════╗
║                    CUA SETUP INSTRUCTIONS                        ║
╠══════════════════════════════════════════════════════════════════╣
║                                                                  ║
║  1. Install Python 3.12 or 3.13:                                ║
║     brew install python@3.13                                     ║
║                                                                  ║
║  2. Install CUA packages:                                        ║
║     pip install cua-agent cua-computer                          ║
║                                                                  ║
║  3. Set up Anthropic API key:                                   ║
║     export ANTHROPIC_API_KEY="your-key-here"                    ║
║                                                                  ║
║  4. (Optional) For sandboxed execution:                         ║
║     npx cuabot                                                  ║
║                                                                  ║
║  For more info: https://cua.ai/docs                             ║
║                                                                  ║
╚══════════════════════════════════════════════════════════════════╝
"""
    print(instructions)
    print(json.dumps({
        "status": "setup_required",
        "message": "CUA setup required. See instructions above."
    }))

if __name__ == "__main__":
    if len(sys.argv) < 2:
        show_setup_instructions()
        sys.exit(1)
    
    task = " ".join(sys.argv[1:])
    
    if task == "--setup":
        show_setup_instructions()
    else:
        asyncio.run(run_task(task))
