# Coucou personalizado — Windows

Este pacote contem CODIGO-FONTE modificado. Nao e um instalador compilado.
O instalador original enviado nao foi alterado. Licencas e creditos originais foram preservados.

## O que foi preparado

- Aplicativos e integracoes como tela inicial da ilha; chat na segunda aba. Pedidos de permissao pendentes continuam tendo prioridade.
- WhatsApp: conversa com mais espaco, fonte maior, horarios e ate 50 mensagens recentes.
  Ao abrir uma conversa, o painel usa sua largura para a leitura; Voltar retorna a lista e as integracoes.
- Botao Abrir midia para fotos, figurinhas, audio e video. Fotos/figurinhas podem ser ampliadas
  dentro do painel; audio/video usam controles nativos. Nao ha reproducao automatica.
  Este recurso de midia usa GOWA; OpenWA continua com as mensagens de texto e rotulos.
  Depende de a mensagem/midia ainda estar disponivel no gateway e de codecs do WebView2.
  Limite de 24 MiB por arquivo. Documentos, envio de anexos e gravacao de audio nao foram implementados.
  O download e solicitado ao GOWA so ao clicar. O GOWA pode manter o arquivo em statics/media;
  a interface do Coucou o mantem em memoria enquanto a conversa esta aberta.
- Controle de arrastar no cabecalho (simbolo de pontos): move horizontalmente e salva a posicao.
  Dois cliques centralizam; com o controle focado, setas movem e Home centraliza.
  Abra a ilha para acessar esse controle. O mascote continua com o gesto proprio dele.
- Importar conversa no chat: escolha .txt ou .md (ate 100 KB). Copie uma conversa ou um resumo
  do ChatGPT para um desses arquivos. O conteudo sera enviado ao provedor selecionado junto
  com sua proxima mensagem, e entra no contexto dessa conversa do Coucou.
  Clique no nome para remover antes de enviar. Limpar o chat tambem remove a importacao pendente.
  Nao ha sincronizacao automatica com o historico do ChatGPT nem acesso por usar a mesma conta.
  Nao importa automaticamente o ZIP/JSON da exportacao completa do ChatGPT.

## Atualizacoes por hotfix — sem reinstalar

As telas ficam embutidas no executavel do Tauri. Por isso ate uma mudanca de interface precisa
ser compilada, mas o usuario recebe um ZIP de hotfix com os binarios necessarios, sem reinstalar
ou refazer login/configuracoes. O app precisa ser fechado e aberto novamente.
Nao e atualizacao instantanea em tempo real nem atualizador remoto automatico.

O aplicador:
1. Verifica os SHA256 dos arquivos do pacote.
2. Localiza a instalacao (ou aceita -InstallDir).
3. Pede para fechar o Coucou.
4. Prepara os novos arquivos e copia os antigos para .coucou-hotfix-backups dentro da instalacao.
5. Substitui apenas coucou.exe e/ou coucou-hook.exe. Nao edita dados de conta, conversas ou configuracoes.
6. Se uma copia falhar, tenta restaurar os arquivos que substituiu.

Extraia o ZIP COMPLETO do hotfix e de dois cliques em Aplicar-Hotfix.cmd.
Se preferir, no PowerShell aberto na pasta extraida:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\Aplicar-Hotfix.ps1
```

O Bypass acima vale somente para esse processo; nao altera a politica permanente do Windows.
Se a instalacao nao for detectada ou houver mais de uma:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\Aplicar-Hotfix.ps1 -InstallDir "C:\caminho\do\Coucou"
```

Instalacoes MSI em uma pasta protegida podem exigir abrir o PowerShell como administrador.
Se houver erro depois de abrir a nova versao, feche o Coucou e copie os .exe do backup mais recente
sobre os arquivos da pasta de instalacao. O rollback automatico cobre falhas de copia, nao falhas
funcionais detectadas depois. Nao apague os backups antes de validar a nova versao.
Os hashes conferem integridade; use apenas pacotes de origem confiavel.
O buscador de atualizacoes consulta somente o repositorio personalizado, evitando oferecer builds originais sobre esta personalizacao. Os pacotes de hotfix ficam em Actions.

## Gerar um hotfix no Windows

Instale Node.js 24 LTS (https://nodejs.org), Rust (https://rustup.rs) e Visual Studio Build Tools
com a carga Desktop development with C++ e Windows SDK. Reinicie o terminal apos instalar.
WebView2 tambem e necessario para executar o aplicativo.

Na pasta windows deste projeto:

```powershell
npm ci
npm run hotfix
```

O pacote fica em windows/release/hotfix-custom-.../. Compacte o conteudo dessa pasta em ZIP.
O comando compila sem gerar instalador NSIS/MSI. Na primeira atualizacao, ele inclui os dois .exe.
Nas seguintes, para incluir apenas os binarios realmente diferentes de uma compilacao anterior:

```powershell
npm run tauri build -- --no-bundle
node scripts/hotfix.mjs --base "C:\pasta\dos\binarios\anteriores"
```

A pasta --base precisa conter os .exe antigos, nao o ZIP deles.
A versao base continua sendo 0.5.9; cada pacote tem uma revisao custom com data no manifesto.
Esse formato deve ser revisto se futuras mudancas adicionarem recursos nativos alem desses dois .exe.

## Alternativa: compilar no GitHub

Em um repositorio seu com este projeto, a acao Coucou - hotfix personalizado pode ser executada
manualmente em Actions. Ela compila no Windows, executa os testes e entrega o artefato
Coucou-Hotfix-Windows. Nao publica releases nem modifica sua instalacao automaticamente.
A primeira execucao pode demorar; seguintes aproveitam o cache de compilacao.
O workflow foi preparado, mas ainda nao executado neste ambiente.

## Validacao

Veja VALIDACAO.txt para os resultados locais e o que ainda depende do Windows.

## Nome personalizado

Em Ajustes > Geral > Nome do assistente, escolha ate 32 caracteres e clique Salvar nome.
O nome aparece na interface, titulos, menu da bandeja e identidade do chat. A chamada de voz
usa OK seguido do nome escolhido (por exemplo, OK Jarvis). O reconhecimento depende do Parakeet
transcrever esse nome; nomes de varias palavras sao aceitos. O nome padrao preserva as variantes
de pronuncia de Coucou. Para perceber a identidade nova no chat, inicie uma conversa nova.

No Windows tambem e criado um alias .cmd com o nome em minusculas (acentos normalizados e
espacos substituidos por hifens), na pasta aliases dos dados do aplicativo. A configuracao mostra
o comando completo para PowerShell. O alias encaminha argumentos, inclusive --shortcut.
Nomes reservados do Windows recebem o prefixo app-. O alias nao e adicionado ao PATH global.
Os nomes tecnicos coucou.exe/coucou-hook.exe, identificador de instalacao e caminhos existentes
continuam estaveis para preservar hotfixes, hooks e dados. Os creditos/licencas originais permanecem.
