import * as vsclang from 'vscode-languageserver'
import { Config } from './config'
import { completions } from './completionProvider';
import { preprocess } from './linter';

const connection = vsclang.createConnection(new vsclang.IPCMessageReader(process), new vsclang.IPCMessageWriter(process));

const documents = new vsclang.TextDocuments();

documents.listen(connection);

const conf = new Config('', '')

connection.onInitialize((params): vsclang.InitializeResult => {
  return {
    capabilities: {
      textDocumentSync: documents.syncKind,
      completionProvider: {
        resolveProvider: true
      },
    }
  };
});

documents.onDidChangeContent((change) => {
  preprocess(change.document);
});

connection.onDidChangeConfiguration((change) => {
  conf.onChange(change.settings as Config)
  documents.all().forEach(preprocess);
});

connection.onCompletion((textDocumentPosition: vsclang.TextDocumentPositionParams): vsclang.CompletionItem[] => {
  return completions
});

connection.onCompletionResolve((item: vsclang.CompletionItem): vsclang.CompletionItem => {
  return completions[item.data - 1]
});

connection.listen();