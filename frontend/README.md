# Frontend (frontend)

Purpose: Minimal Next.js + TypeScript app that demonstrates connectivity to the API Gateway.

How to run:

```bash
cd frontend
npm install
npm run dev
```

Open http://localhost:3000 to view the test page which calls the API Gateway `/graphql` endpoint.

User stories covered:
- View recent articles filtered by region and language (placeholder UI)
- Search articles by keywords or categories (to be implemented)
- Mark articles as favorites locally (client-side storage)

Prompting template (when requesting frontend features):
```
You are an expert in React + Next.js (App Router) with TypeScript and Tailwind CSS.
Implement [component] using SWR for data fetching, fetch GraphQL queries from /graphql,
and follow accessibility and responsive design best practices.
```
