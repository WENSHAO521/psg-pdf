%% psg-article.cls -- research article class for Panorama Scholarly Group journals.
%% Part of PSG Academic PDF. Licensed MIT OR Apache-2.0, like the rest of the repository.
%% Typeset with XeLaTeX (Chinese and Japanese through xeCJK) or pdfLaTeX (Western text only);
%% the bibliography uses BibLaTeX with the Biber backend.
\NeedsTeXFormat{LaTeX2e}
\ProvidesClass{psg-article}[2026/11/20 v1.3.0 PSG research article]

%% Options. `authoryear`: author-year citations (Humanities and Social Sciences template) instead
%% of numbered ones.
\newif\ifpsg@authoryear
\DeclareOption{authoryear}{\psg@authoryeartrue}
\DeclareOption*{\ClassWarning{psg-article}{Unknown option `\CurrentOption' ignored}}
\ProcessOptions\relax

\LoadClass[10pt,a4paper,twoside]{article}

%% Fonts. XeLaTeX: Latin Modern through fontspec, and for Chinese and Japanese the Noto CJK faces
%% PSG Academic PDF installs (OFL-1.1), found by file name: Noto Serif TC for running text with
%% Noto Serif SC for characters only Simplified Chinese uses, Noto Sans for sans-serif. Without
%% them, a Noto or Source Han face installed on the computer is used. pdfLaTeX: Latin Modern.
\RequirePackage{iftex}
\ifXeTeX
  \RequirePackage{fontspec}
  \RequirePackage[AutoFallBack=true]{xeCJK}
  \xeCJKsetup{CJKspace=false}
  \newif\ifpsg@cjk
  \IfFontExistsTF{NotoSerifTC-Regular.otf}{%
    \psg@cjktrue
    \setCJKmainfont{NotoSerifTC-Regular.otf}[BoldFont=NotoSerifTC-Bold.otf]
    \setCJKsansfont{NotoSansTC-Regular.otf}[BoldFont=NotoSansTC-Bold.otf]
    \setCJKmonofont{NotoSansTC-Regular.otf}
    \IfFontExistsTF{NotoSerifSC-Regular.otf}{%
      \setCJKfallbackfamilyfont{\CJKrmdefault}[BoldFont=NotoSerifSC-Bold.otf]{NotoSerifSC-Regular.otf}
      \setCJKfallbackfamilyfont{\CJKsfdefault}[BoldFont=NotoSansSC-Bold.otf]{NotoSansSC-Regular.otf}}{}%
  }{}
  \ifpsg@cjk\else
    \IfFontExistsTF{Noto Serif CJK TC}{\psg@cjktrue
      \setCJKmainfont{Noto Serif CJK TC}\setCJKsansfont{Noto Sans CJK TC}\setCJKmonofont{Noto Sans CJK TC}}{%
    \IfFontExistsTF{Source Han Serif TC}{\psg@cjktrue
      \setCJKmainfont{Source Han Serif TC}\setCJKsansfont{Source Han Sans TC}\setCJKmonofont{Source Han Sans TC}}{}}
  \fi
  \ifpsg@cjk\else
    \ClassWarningNoLine{psg-article}{No Chinese or Japanese font was found. PSG Academic PDF
      installs Noto CJK with its TeX (Settings > TeX installation)}
  \fi
\else
  \RequirePackage[T1]{fontenc}
  \RequirePackage{lmodern}
\fi
\RequirePackage{microtype}

%% Page layout.
\RequirePackage[a4paper,top=25mm,bottom=25mm,inner=22mm,outer=22mm,headsep=8mm,footskip=10mm]{geometry}

%% Common content packages, so converted manuscripts compile without extra setup.
\RequirePackage{amsmath,amssymb}
\RequirePackage{graphicx}
\RequirePackage{booktabs}
\RequirePackage{longtable}
\RequirePackage{array}
\RequirePackage{calc}
\RequirePackage[table]{xcolor}
\RequirePackage[font=small,labelfont=bf]{caption}
\RequirePackage{fancyhdr}

%% Bibliography: BibLaTeX with Biber.
\RequirePackage{csquotes}
\ifpsg@authoryear
  \RequirePackage[backend=biber,style=authoryear-comp,maxcitenames=2,maxbibnames=10,uniquelist=false,dashed=false]{biblatex}
\else
  \RequirePackage[backend=biber,style=numeric-comp,sorting=none,maxbibnames=10]{biblatex}
\fi

\RequirePackage[hidelinks,unicode]{hyperref}

\definecolor{psgaccent}{RGB}{31,58,104}

%% Commands Pandoc's LaTeX writer uses in a converted Word manuscript (DOCX import), so the
%% converted body compiles as it is.
\providecommand{\tightlist}{\setlength{\itemsep}{0pt}\setlength{\parskip}{0pt}}
\providecommand{\passthrough}[1]{#1}
%% Pandoc 3.2+ wraps images in \pandocbounded: here, shrink one that is wider than the line or
%% taller than the text area, keeping its proportions; never enlarge it.
\newsavebox\psg@figbox
\providecommand{\pandocbounded}[1]{%
  \sbox\psg@figbox{#1}%
  \ifdim\wd\psg@figbox>\linewidth\sbox\psg@figbox{\resizebox{\linewidth}{!}{\usebox\psg@figbox}}\fi
  \ifdim\dimexpr\ht\psg@figbox+\dp\psg@figbox\relax>0.9\textheight
    \sbox\psg@figbox{\resizebox{!}{0.9\textheight}{\usebox\psg@figbox}}\fi
  \usebox\psg@figbox}
%% Strikeout and underline: with soul when TeX Live has it, otherwise plain and underlined text.
\newcommand\psg@plainst[1]{#1}
\newcommand\psg@plainul[1]{\underline{#1}}
\IfFileExists{soul.sty}{\RequirePackage{soul}}{\let\st\psg@plainst\let\ul\psg@plainul}

%% ---------------------------------------------------------------------------
%% Publication metadata. PSG Academic PDF writes psg-metadata.tex, which calls
%% these setters; every value has already been escaped for LaTeX.
%% ---------------------------------------------------------------------------
\newcommand*\psg@journal{PSG Journal}
\newcommand*\psg@journalabbrev{}
\newcommand*\psg@issn{}
\newcommand*\psg@volume{}
\newcommand*\psg@issue{}
\newcommand*\psg@year{}
\newcommand*\psg@pages{}
\newcommand*\psg@articlenumber{}
\newcommand*\psg@doi{}
\newcommand*\psg@articletype{Research Article}
\newcommand*\psg@title{Untitled}
\newcommand*\psg@shorttitle{}
\newcommand\psg@authors{}
\newcommand\psg@affiliations{}
\newcommand\psg@correspondence{}
\newcommand\psg@abstract{}
\newcommand\psg@keywords{}
\newcommand*\psg@received{}
\newcommand*\psg@revised{}
\newcommand*\psg@accepted{}
\newcommand*\psg@published{}
\newcommand\psg@licence{}
\newcommand\psg@copyright{}
\newcommand*\psg@pdftitle{}
\newcommand*\psg@pdfauthor{}
\newcommand*\psg@pdfkeywords{}

\newcommand\psgJournal[1]{\renewcommand*\psg@journal{#1}}
\newcommand\psgJournalAbbrev[1]{\renewcommand*\psg@journalabbrev{#1}}
\newcommand\psgISSN[1]{\renewcommand*\psg@issn{#1}}
\newcommand\psgVolume[1]{\renewcommand*\psg@volume{#1}}
\newcommand\psgIssue[1]{\renewcommand*\psg@issue{#1}}
\newcommand\psgYear[1]{\renewcommand*\psg@year{#1}}
\newcommand\psgPages[1]{\renewcommand*\psg@pages{#1}}
\newcommand\psgArticleNumber[1]{\renewcommand*\psg@articlenumber{#1}}
\newcommand\psgDOI[1]{\renewcommand*\psg@doi{#1}}
\newcommand\psgArticleType[1]{\renewcommand*\psg@articletype{#1}}
\newcommand\psgTitle[1]{\renewcommand*\psg@title{#1}}
\newcommand\psgShortTitle[1]{\renewcommand*\psg@shorttitle{#1}}
\newcommand\psgAuthors[1]{\renewcommand\psg@authors{#1}}
\newcommand\psgAffiliations[1]{\renewcommand\psg@affiliations{#1}}
\newcommand\psgCorrespondence[1]{\renewcommand\psg@correspondence{#1}}
\newcommand\psgAbstract[1]{\renewcommand\psg@abstract{#1}}
\newcommand\psgKeywords[1]{\renewcommand\psg@keywords{#1}}
\newcommand\psgReceived[1]{\renewcommand*\psg@received{#1}}
\newcommand\psgRevised[1]{\renewcommand*\psg@revised{#1}}
\newcommand\psgAccepted[1]{\renewcommand*\psg@accepted{#1}}
\newcommand\psgPublished[1]{\renewcommand*\psg@published{#1}}
\newcommand\psgLicence[1]{\renewcommand\psg@licence{#1}}
\newcommand\psgCopyright[1]{\renewcommand\psg@copyright{#1}}
\newcommand\psgPdfTitle[1]{\renewcommand*\psg@pdftitle{#1}}
\newcommand\psgPdfAuthor[1]{\renewcommand*\psg@pdfauthor{#1}}
\newcommand\psgPdfKeywords[1]{\renewcommand*\psg@pdfkeywords{#1}}

%% The article's language, from the metadata (\psgLanguage{zh-hant}, zh-hans, ja or en): the
%% labels of the title block, figures, tables and the bibliography follow it.
\newcommand*\psg@lbl@abstract{Abstract.}
\newcommand*\psg@lbl@keywords{Keywords:}
\newcommand*\psg@lbl@received{Received:}
\newcommand*\psg@lbl@revised{Revised:}
\newcommand*\psg@lbl@accepted{Accepted:}
\newcommand*\psg@lbl@published{Published:}
\newcommand*\psg@lbl@figure{Figure}
\newcommand*\psg@lbl@table{Table}
\newcommand*\psg@lbl@references{References}
\newcommand\psg@labels[9]{%
  \renewcommand*\psg@lbl@abstract{#1}\renewcommand*\psg@lbl@keywords{#2}%
  \renewcommand*\psg@lbl@received{#3}\renewcommand*\psg@lbl@revised{#4}%
  \renewcommand*\psg@lbl@accepted{#5}\renewcommand*\psg@lbl@published{#6}%
  \renewcommand*\psg@lbl@figure{#7}\renewcommand*\psg@lbl@table{#8}%
  \renewcommand*\psg@lbl@references{#9}}
\def\psg@lang@zhhant{zh-hant}
\def\psg@lang@zhhans{zh-hans}
\def\psg@lang@ja{ja}
\newcommand\psgLanguage[1]{%
  \edef\psg@lang{#1}%
  \ifx\psg@lang\psg@lang@zhhant
    \psg@labels{摘要：}{關鍵詞：}{收稿日期：}{修訂日期：}{接受日期：}{出版日期：}{圖}{表}{參考文獻}\fi
  \ifx\psg@lang\psg@lang@zhhans
    \psg@labels{摘要：}{关键词：}{收稿日期：}{修订日期：}{接受日期：}{出版日期：}{图}{表}{参考文献}\fi
  \ifx\psg@lang\psg@lang@ja
    \psg@labels{要旨：}{キーワード：}{受付：}{改訂：}{受理：}{公開：}{図}{表}{参考文献}\fi
  \DefineBibliographyStrings{english}{references={\psg@lbl@references},bibliography={\psg@lbl@references}}%
  % Chinese and Japanese: every paragraph indented two characters, and a space after "圖 1".
  \ifx\psg@lbl@figure\psg@lbl@en@figure\else
    \setlength\parindent{2em}\RequirePackage{indentfirst}\captionsetup{labelsep=quad}%
  \fi}
\newcommand*\psg@lbl@en@figure{Figure}
\AtBeginDocument{%
  \renewcommand*\figurename{\psg@lbl@figure}%
  \renewcommand*\tablename{\psg@lbl@table}}

%% Author superscripts, used by the generated author list.
\newcommand\psgAffMark[1]{\textsuperscript{#1}}

\AtBeginDocument{%
  \hypersetup{pdftitle={\psg@pdftitle},pdfauthor={\psg@pdfauthor},pdfkeywords={\psg@pdfkeywords},pdfsubject={\psg@journal}}%
}

%% Citation line used in the running header and the title block.
\newcommand\psg@citeline{%
  \textit{\psg@journal}%
  \ifx\psg@year\@empty\else\space\psg@year\fi
  \ifx\psg@volume\@empty\else, \textbf{\psg@volume}\fi
  \ifx\psg@issue\@empty\else(\psg@issue)\fi
  \ifx\psg@pages\@empty\else, \psg@pages\fi
  \ifx\psg@articlenumber\@empty\else, \psg@articlenumber\fi}

%% Running headers and footers.
\pagestyle{fancy}
\fancyhf{}
\renewcommand{\headrulewidth}{0.4pt}
\fancyhead[LE]{\small\psg@citeline}
\fancyhead[RO]{\small\ifx\psg@shorttitle\@empty\psg@title\else\psg@shorttitle\fi}
\fancyhead[LO,RE]{\small\thepage}
\fancyfoot[C]{\footnotesize\ifx\psg@doi\@empty\else\href{https://doi.org/\psg@doi}{https://doi.org/\psg@doi}\fi}
% No \par in a fancyhdr argument: fancyhdr before 4.0 (TeX Live 2021) reads it as a short macro.
\fancypagestyle{psgfirst}{%
  \fancyhf{}%
  \renewcommand{\headrulewidth}{0pt}%
  \fancyfoot[C]{\parbox[b]{\textwidth}{\footnotesize\raggedright
    \ifx\psg@copyright\@empty\else\psg@copyright\\\fi
    \psg@licence}}%
}

%% Section headings in the accent colour.
\renewcommand\section{\@startsection{section}{1}{\z@}%
  {-3ex \@plus -1ex \@minus -.2ex}{1.5ex \@plus .2ex}%
  {\normalfont\large\bfseries\color{psgaccent}}}
\renewcommand\subsection{\@startsection{subsection}{2}{\z@}%
  {-2.5ex \@plus -1ex \@minus -.2ex}{1ex \@plus .2ex}%
  {\normalfont\normalsize\bfseries\color{psgaccent}}}

%% Whether a metadata value (a macro) is empty, however it was defined: \psg@ifempty\macro{yes}{no}.
\newcommand\psg@ifempty[1]{%
  \if\relax\detokenize\expandafter{#1}\relax\expandafter\@firstoftwo\else\expandafter\@secondoftwo\fi}
%% The abstract and keywords, each only when there is one.
\newcommand\psg@abstractbox{%
  \noindent\colorbox{psgaccent!6}{\begin{minipage}{\textwidth-2\fboxsep}
    \small\psg@ifempty\psg@abstract{}{\textbf{\psg@lbl@abstract}\space\psg@abstract}%
    \psg@ifempty\psg@keywords{}{\psg@ifempty\psg@abstract{}{\par\smallskip}\textbf{\psg@lbl@keywords}\space\psg@keywords}%
  \end{minipage}}\par}

%% The title block.
\newcommand\psgmaketitle{%
  \thispagestyle{psgfirst}%
  \begingroup
  \noindent{\color{psgaccent}\rule{\textwidth}{2pt}}\par\vspace{2pt}%
  \noindent{\small\textsc{\psg@journal}%
    \ifx\psg@issn\@empty\else\hfill ISSN \psg@issn\fi}\par
  \noindent{\small\psg@citeline}\par
  \ifx\psg@doi\@empty\else\noindent{\small DOI: \href{https://doi.org/\psg@doi}{\psg@doi}}\par\fi
  \vspace{10pt}%
  \noindent{\small\color{psgaccent}\MakeUppercase{\psg@articletype}}\par\vspace{4pt}%
  \noindent{\LARGE\bfseries\raggedright\psg@title\par}%
  \vspace{10pt}%
  \noindent{\normalsize\raggedright\psg@authors\par}%
  \vspace{6pt}%
  \noindent\begin{minipage}{\textwidth}\footnotesize\raggedright\psg@affiliations
    \ifx\psg@correspondence\@empty\else\par\smallskip\psg@correspondence\fi\end{minipage}\par
  \vspace{8pt}%
  \noindent\begin{minipage}{\textwidth}\footnotesize
    \ifx\psg@received\@empty\else \psg@lbl@received\space\psg@received\quad\fi
    \ifx\psg@revised\@empty\else \psg@lbl@revised\space\psg@revised\quad\fi
    \ifx\psg@accepted\@empty\else \psg@lbl@accepted\space\psg@accepted\quad\fi
    \ifx\psg@published\@empty\else \psg@lbl@published\space\psg@published\fi
  \end{minipage}\par
  \vspace{10pt}%
  \psg@ifempty\psg@abstract{\psg@ifempty\psg@keywords{}{\psg@abstractbox}}{\psg@abstractbox}%
  \vspace{14pt}%
  \endgroup}

\endinput
