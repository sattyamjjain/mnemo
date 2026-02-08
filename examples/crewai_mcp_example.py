"""Example: CrewAI + Mnemo via MCP.

CrewAI agents connect to Mnemo via MCPServerAdapter, giving entire
crews access to shared persistent memory. This complements CrewAI's
built-in memory with Mnemo's versioned, searchable, multi-agent memory.

Requirements:
    pip install crewai 'crewai-tools[mcp]'
    cargo build --release -p mnemo-cli
    export OPENAI_API_KEY=sk-...
"""

from crewai import Agent, Task, Crew, Process
from crewai_tools import MCPServerAdapter
from mcp import StdioServerParameters

# Configure Mnemo MCP server
server_params = StdioServerParameters(
    command="mnemo",
    args=["--db-path", "crewai_demo.db", "--agent-id", "crew-shared"],
)


def main():
    # Connect to Mnemo via MCP
    with MCPServerAdapter(server_params) as mnemo_tools:

        # Researcher agent with memory tools
        researcher = Agent(
            role="Senior Researcher",
            goal="Research topics and store findings in persistent memory",
            backstory="You are an expert researcher who saves all findings to memory.",
            tools=mnemo_tools,
            verbose=True,
        )

        # Analyst agent with memory tools (shared memory)
        analyst = Agent(
            role="Data Analyst",
            goal="Analyze research from memory and produce insights",
            backstory="You retrieve stored research and add your analysis.",
            tools=mnemo_tools,
            verbose=True,
        )

        # Task 1: Research and store
        research_task = Task(
            description=(
                "Research the current state of AI agent memory systems. "
                "Store each key finding using mnemo.remember with appropriate tags. "
                "Include: market size, key players, and technical approaches."
            ),
            expected_output="List of stored memory IDs with summaries.",
            agent=researcher,
        )

        # Task 2: Recall and analyze
        analysis_task = Task(
            description=(
                "Use mnemo.recall to retrieve all stored research about AI memory. "
                "Analyze the findings and produce a strategic summary. "
                "Store your analysis back to memory with tag 'analysis'."
            ),
            expected_output="Strategic analysis based on recalled research.",
            agent=analyst,
        )

        # Create and run the crew
        crew = Crew(
            agents=[researcher, analyst],
            tasks=[research_task, analysis_task],
            process=Process.sequential,
            verbose=True,
        )

        result = crew.kickoff()
        print(f"\n=== Final Result ===\n{result}")


# Alternative: Using the mcps field (CrewAI 1.9+)
def with_mcps_field():
    agent = Agent(
        role="Memory Agent",
        goal="Use persistent memory",
        backstory="You have access to Mnemo memory.",
        mcps=["mnemo --db-path crewai_demo.db"],
    )

    task = Task(
        description="Remember that the project deadline is March 15th.",
        expected_output="Confirmation of stored memory.",
        agent=agent,
    )

    crew = Crew(agents=[agent], tasks=[task])
    crew.kickoff()


if __name__ == "__main__":
    main()
