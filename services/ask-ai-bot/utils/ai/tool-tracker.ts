export class ToolTracker {
  private docSearchCalls: number = 0;
  private docSearchResults: Set<string> = new Set();
  private viewedDocPaths: Set<string> = new Set();
  private codeSearchCalls: number = 0;
  private codeSearchResults: number = 0;
  private viewedCodePaths: Set<string> = new Set();
  private listedDirs: number = 0;
  private gitHubSearchCalls: number = 0;
  private gitHubReadCalls: number = 0;

  recordSearchCall(results: string[]): void {
    this.docSearchCalls++;
    results.forEach((result) => this.docSearchResults.add(result));
  }

  recordViewCall(filePaths: string | string[]): void {
    const paths = Array.isArray(filePaths) ? filePaths : [filePaths];
    paths.forEach((path) => this.viewedDocPaths.add(path));
  }

  recordCodeSearchCall(resultCount: number): void {
    this.codeSearchCalls++;
    this.codeSearchResults += resultCount;
  }

  recordCodeViewCall(filePaths: string | string[]): void {
    const paths = Array.isArray(filePaths) ? filePaths : [filePaths];
    paths.forEach((path) => this.viewedCodePaths.add(path));
  }

  recordListDir(): void {
    this.listedDirs++;
  }

  recordGitHubSearch(): void {
    this.gitHubSearchCalls++;
  }

  recordGitHubRead(): void {
    this.gitHubReadCalls++;
  }

  getSummary(): string {
    const parts: string[] = [];

    if (this.docSearchCalls > 0) {
      const resultCount = this.docSearchResults.size;
      const timesText = this.docSearchCalls === 1 ? "time" : "times";
      const resultsText = resultCount === 1 ? "result" : "results";
      parts.push(
        `searched docs ${this.docSearchCalls} ${timesText} with ${resultCount} ${resultsText}`,
      );
    }

    if (this.viewedDocPaths.size > 0) {
      const pageCount = this.viewedDocPaths.size;
      const pagesText = pageCount === 1 ? "page" : "pages";
      parts.push(`viewed ${pageCount} doc ${pagesText}`);
    }

    if (this.codeSearchCalls > 0) {
      const timesText = this.codeSearchCalls === 1 ? "time" : "times";
      const matchText = this.codeSearchResults === 1 ? "match" : "matches";
      parts.push(
        `searched code ${this.codeSearchCalls} ${timesText} with ${this.codeSearchResults} ${matchText}`,
      );
    }

    if (this.viewedCodePaths.size > 0) {
      const fileCount = this.viewedCodePaths.size;
      const filesText = fileCount === 1 ? "file" : "files";
      parts.push(`viewed ${fileCount} source ${filesText}`);
    }

    if (this.listedDirs > 0) {
      const timesText = this.listedDirs === 1 ? "time" : "times";
      parts.push(`listed directories ${this.listedDirs} ${timesText}`);
    }

    if (this.gitHubSearchCalls > 0) {
      const timesText = this.gitHubSearchCalls === 1 ? "time" : "times";
      parts.push(`searched GitHub ${this.gitHubSearchCalls} ${timesText}`);
    }

    if (this.gitHubReadCalls > 0) {
      const itemText = this.gitHubReadCalls === 1 ? "item" : "items";
      parts.push(`read ${this.gitHubReadCalls} GitHub ${itemText}`);
    }

    if (parts.length === 0) return "";

    const firstPart = parts[0].charAt(0).toUpperCase() + parts[0].slice(1);
    if (parts.length === 1) return firstPart;
    return firstPart + ", " + parts.slice(1).join(", ");
  }
}
