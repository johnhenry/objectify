import { DoBase } from './_dobase.ts';

interface Task {
  id: string;
  title: string;
  done: boolean;
  createdAt: string;
}

interface TaskListState {
  tasks: Task[];
}

export default class TaskList extends DoBase<TaskListState> {
  add = async ({ title }: { title: string }) => {
    const { tasks = [] } = (await this.get()) || {};
    const task: Task = {
      id: crypto.randomUUID(),
      title,
      done: false,
      createdAt: new Date().toISOString(),
    };
    await this.set({ tasks: [...tasks, task] });
    return task;
  };

  complete = async ({ id }: { id: string }) => {
    const { tasks = [] } = (await this.get()) || {};
    await this.set({
      tasks: tasks.map(t => (t.id === id ? { ...t, done: true } : t)),
    });
  };

  pending = async () => {
    const { tasks = [] } = (await this.get()) || {};
    return tasks.filter(t => !t.done);
  };

  done = async () => {
    const { tasks = [] } = (await this.get()) || {};
    return tasks.filter(t => t.done);
  };

  clear = async () => {
    await this.set({ tasks: [] });
  };
}
