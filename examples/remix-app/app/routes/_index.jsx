import { json } from "@remix-run/node";
import { useLoaderData, Form } from "@remix-run/react";

const todos = [
  { id: 1, title: "Try Remix on Howth", done: true },
  { id: 2, title: "Build something cool", done: false },
  { id: 3, title: "Read the docs", done: false },
];
let nextId = 4;

export async function loader() {
  return json({ todos });
}

export async function action({ request }) {
  const formData = await request.formData();
  const intent = formData.get("intent");

  if (intent === "add") {
    const title = formData.get("title");
    if (title) {
      todos.push({ id: nextId++, title, done: false });
    }
  } else if (intent === "toggle") {
    const id = parseInt(formData.get("id"), 10);
    const todo = todos.find((t) => t.id === id);
    if (todo) {
      todo.done = !todo.done;
    }
  } else if (intent === "delete") {
    const id = parseInt(formData.get("id"), 10);
    const idx = todos.findIndex((t) => t.id === id);
    if (idx !== -1) {
      todos.splice(idx, 1);
    }
  }

  return json({ todos });
}

export default function Index() {
  const { todos } = useLoaderData();

  return (
    <>
      <h1>Remix on Howth</h1>
      <p className="subtitle">
        A full-stack React framework running on the Howth runtime
      </p>

      <div className="card">
        <h2>
          Todo List <span className="badge">{todos.length}</span>
        </h2>

        <Form method="post">
          <div className="todo-form">
            <input type="hidden" name="intent" value="add" />
            <input type="text" name="title" placeholder="Add a new todo..." />
            <button type="submit">Add</button>
          </div>
        </Form>

        <ul>
          {todos.map((todo) => (
            <li key={todo.id} className={todo.done ? "done" : ""}>
              <Form method="post" style={{ display: "inline" }}>
                <input type="hidden" name="intent" value="toggle" />
                <input type="hidden" name="id" value={todo.id} />
                <button
                  type="submit"
                  style={{
                    background: "none",
                    color: "#4f46e5",
                    padding: "0",
                    cursor: "pointer",
                    textDecoration: todo.done ? "line-through" : "none",
                  }}
                >
                  {todo.title}
                </button>
              </Form>
              {todo.done ? " \u2713" : ""}
              <Form
                method="post"
                style={{ display: "inline", marginLeft: "0.5rem" }}
              >
                <input type="hidden" name="intent" value="delete" />
                <input type="hidden" name="id" value={todo.id} />
                <button
                  type="submit"
                  style={{
                    background: "none",
                    color: "#999",
                    padding: "0",
                    fontSize: "0.8rem",
                  }}
                >
                  x
                </button>
              </Form>
            </li>
          ))}
        </ul>
      </div>
    </>
  );
}
